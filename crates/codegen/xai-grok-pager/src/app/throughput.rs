//! In-memory tps/rpm for the session on screen.
//!
//! The shell reports each model request when it is sent and when it finishes.
//! Streamed text and reasoning that this process actually receives fill the live
//! estimate. A finished call replaces that estimate with the server token count
//! over the shell's first-token-to-end interval. Nothing is written to disk.

use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

/// Requests older than this drop out of `rpm`.
pub const RPM_WINDOW: Duration = Duration::from_secs(60);

/// One shown sample. `tps` is absent until the owning call has produced a token.
#[derive(Debug, Clone, PartialEq)]
pub struct Labels {
    pub tps: Option<TpsLabel>,
    pub rpm: Option<String>,
}

impl Labels {
    pub fn is_empty(&self) -> bool {
        self.tps.is_none() && self.rpm.is_none()
    }
}

impl Default for Labels {
    fn default() -> Self {
        Self {
            tps: None,
            rpm: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TpsLabel {
    pub text: String,
    /// The number actually drawn, after the one-decimal / integer rule.
    pub shown: f64,
    pub warn: bool,
}

#[derive(Debug, Clone)]
struct Call {
    chars: u64,
    first_token_at: Option<Instant>,
    /// This call may paint tps. A background call loses this while a user call is in flight.
    owns: bool,
}

/// Per-session counters. One view, one tracker.
#[derive(Debug, Default, Clone)]
pub struct Throughput {
    requests: VecDeque<Instant>,
    calls: HashMap<String, Call>,
    user_owner: Option<String>,
    background_owner: Option<String>,
    /// Background calls that started while a user call owned tps, oldest first.
    waiting_background: VecDeque<String>,
    retained_tps: Option<f64>,
}

impl Throughput {
    pub fn start(&mut self, id: &str, user_turn: bool, now: Instant) {
        self.prune(now);
        self.requests.push_back(now);
        if self.calls.contains_key(id) {
            // A retry of the same request. Count it again and wait for its first token.
            let call = self.calls.get_mut(id).expect("just checked");
            call.chars = 0;
            call.first_token_at = None;
            if call.owns {
                self.retained_tps = None;
            }
            return;
        }
        let owns = if user_turn {
            self.disown_user();
            self.disown_background();
            self.user_owner = Some(id.to_string());
            true
        } else if self.user_owner.is_none() {
            self.disown_background();
            self.background_owner = Some(id.to_string());
            true
        } else {
            self.waiting_background.push_back(id.to_string());
            false
        };
        if owns {
            self.retained_tps = None;
        }
        self.calls.insert(
            id.to_string(),
            Call {
                chars: 0,
                first_token_at: None,
                owns,
            },
        );
    }

    /// Bytes of text or reasoning that arrived for whichever call currently owns tps.
    pub fn add_chars(&mut self, chars: u64, now: Instant) {
        if chars == 0 {
            return;
        }
        let Some(id) = self
            .user_owner
            .clone()
            .or_else(|| self.background_owner.clone())
        else {
            return;
        };
        let Some(call) = self.calls.get_mut(&id) else {
            return;
        };
        if !call.owns {
            return;
        }
        call.chars = call.chars.saturating_add(chars);
        if call.first_token_at.is_none() {
            call.first_token_at = Some(now);
        }
    }

    pub fn finish(
        &mut self,
        id: &str,
        output_tokens: Option<u64>,
        decode_ms: Option<u64>,
        now: Instant,
    ) {
        self.prune(now);
        let Some(call) = self.calls.remove(id) else {
            return;
        };
        let was_user = self.user_owner.as_deref() == Some(id);
        let was_background = self.background_owner.as_deref() == Some(id);
        if was_user {
            self.user_owner = None;
        }
        if was_background {
            self.background_owner = None;
        }
        self.waiting_background.retain(|waiting| waiting != id);
        if call.owns {
            self.retained_tps = rate_of(&call, output_tokens, decode_ms, now);
        }
        if was_user {
            self.promote_background();
        }
    }

    pub fn labels(&self, now: Instant, warn_below: f64) -> Labels {
        let rpm = self.rpm_at(now);
        let live = self.live_tps(now);
        let tps_rate = live.or(self.retained_tps);
        if tps_rate.is_none() && rpm == 0 {
            return Labels::default();
        }
        let tps = tps_rate.map(|rate| {
            let (text, shown) = format_tps(rate);
            TpsLabel {
                warn: warn_below > 0.0 && shown < warn_below,
                text,
                shown,
            }
        });
        Labels {
            rpm: Some(format!("{rpm} rpm")),
            tps,
        }
    }

    fn rpm_at(&self, now: Instant) -> usize {
        self.requests
            .iter()
            .filter(|sent| now.saturating_duration_since(**sent) <= RPM_WINDOW)
            .count()
    }

    fn live_tps(&self, now: Instant) -> Option<f64> {
        let id = self
            .user_owner
            .as_ref()
            .or(self.background_owner.as_ref())?;
        let call = self.calls.get(id)?;
        if !call.owns {
            return None;
        }
        let started = call.first_token_at?;
        let tokens = estimated_tokens(call.chars)?;
        let secs = now
            .saturating_duration_since(started)
            .as_secs_f64()
            .max(0.001);
        Some(tokens as f64 / secs)
    }

    fn prune(&mut self, now: Instant) {
        while self
            .requests
            .front()
            .is_some_and(|sent| now.saturating_duration_since(*sent) > RPM_WINDOW)
        {
            self.requests.pop_front();
        }
    }

    fn disown_user(&mut self) {
        if let Some(prev) = self.user_owner.take()
            && let Some(call) = self.calls.get_mut(&prev)
        {
            call.owns = false;
        }
    }

    fn disown_background(&mut self) {
        if let Some(prev) = self.background_owner.take()
            && let Some(call) = self.calls.get_mut(&prev)
        {
            call.owns = false;
            self.waiting_background.push_back(prev);
        }
    }

    /// The newest background call still running becomes the tps owner once the user call is gone.
    fn promote_background(&mut self) {
        if self.user_owner.is_some() || self.background_owner.is_some() {
            return;
        }
        while let Some(id) = self.waiting_background.pop_back() {
            if self.calls.contains_key(&id) {
                if let Some(call) = self.calls.get_mut(&id) {
                    call.owns = true;
                    if call.first_token_at.is_none() {
                        self.retained_tps = None;
                    }
                }
                self.background_owner = Some(id);
                return;
            }
        }
    }
}

fn estimated_tokens(chars: u64) -> Option<u64> {
    if chars == 0 {
        return None;
    }
    // Same bytes/4 heuristic as the rest of the client. A short chunk still counts as one token
    // so the live number is not stuck at nothing until four bytes have arrived.
    Some((chars / xai_token_estimation::BYTES_PER_TOKEN).max(1))
}

fn rate_of(
    call: &Call,
    output_tokens: Option<u64>,
    decode_ms: Option<u64>,
    now: Instant,
) -> Option<f64> {
    let tokens = match output_tokens {
        Some(n) => n,
        None => estimated_tokens(call.chars).unwrap_or(0),
    };
    if tokens == 0 {
        return None;
    }
    let secs = if let Some(ms) = decode_ms {
        ms as f64 / 1000.0
    } else {
        let started = call.first_token_at?;
        now.saturating_duration_since(started).as_secs_f64()
    };
    Some(tokens as f64 / secs.max(0.001))
}

/// `9.96` draws as `10 tps`. Below 10, one decimal. Half rounds away from zero.
pub fn format_tps(rate: f64) -> (String, f64) {
    if !rate.is_finite() || rate < 0.0 {
        return ("0.0 tps".to_string(), 0.0);
    }
    let tenths = (rate * 10.0 + 0.5).floor() as u64;
    if tenths >= 100 {
        let whole = if tenths % 10 >= 5 {
            tenths / 10 + 1
        } else {
            tenths / 10
        };
        (format!("{whole} tps"), whole as f64)
    } else {
        let whole = tenths / 10;
        let frac = tenths % 10;
        (
            format!("{whole}.{frac} tps"),
            whole as f64 + frac as f64 / 10.0,
        )
    }
}

/// Bytes of visible text or reasoning in one ACP chunk. Tool-call argument text is counted separately.
pub(crate) fn streamed_output_bytes(update: &agent_client_protocol::SessionUpdate) -> Option<u64> {
    let chunk = match update {
        agent_client_protocol::SessionUpdate::AgentMessageChunk(chunk)
        | agent_client_protocol::SessionUpdate::AgentThoughtChunk(chunk) => chunk,
        _ => return None,
    };
    match &chunk.content {
        agent_client_protocol::ContentBlock::Text(text) => Some(text.text.len() as u64),
        _ => None,
    }
}

pub(crate) fn note_streamed_output(
    agent: &mut crate::app::agent_view::AgentView,
    update: &agent_client_protocol::SessionUpdate,
    is_replay: bool,
) {
    if is_replay || agent.session.loading_replay {
        return;
    }
    let Some(chars) = streamed_output_bytes(update) else {
        return;
    };
    agent.throughput.add_chars(chars, std::time::Instant::now());
}

/// `None` and any negative or non-finite value use 10. `0` stays 0 and turns the warning color off.
pub fn resolve_warn_tps(raw: Option<f64>) -> f64 {
    match raw {
        None => 10.0,
        Some(value) if value.is_finite() && value >= 0.0 => value,
        Some(_) => 10.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Instant {
        Instant::now()
    }

    fn t() -> Throughput {
        Throughput::default()
    }

    #[test]
    fn nothing_sent_draws_nothing() {
        let labels = t().labels(base(), 10.0);
        assert!(labels.is_empty());
    }

    #[test]
    fn sent_request_shows_rpm_until_the_first_token() {
        let mut tp = t();
        let now = base();
        tp.start("a", true, now);
        let labels = tp.labels(now, 10.0);
        assert!(labels.tps.is_none());
        assert_eq!(labels.rpm.as_deref(), Some("1 rpm"));
    }

    #[test]
    fn live_rate_uses_arrived_text_then_server_tokens_replace_it() {
        let mut tp = t();
        let now = base();
        tp.start("a", true, now);
        tp.add_chars(40, now + Duration::from_millis(100));
        let live = tp.labels(now + Duration::from_millis(1_100), 10.0);
        let tps = live.tps.expect("live tps");
        // 40 bytes -> 10 tokens over 1.0s.
        assert_eq!(tps.text, "10 tps");
        assert!(!tps.warn);

        tp.finish(
            "a",
            Some(5),
            Some(1_000),
            now + Duration::from_millis(1_100),
        );
        let done = tp.labels(now + Duration::from_millis(1_100), 10.0);
        assert_eq!(done.tps.unwrap().text, "5.0 tps");
        assert_eq!(done.rpm.as_deref(), Some("1 rpm"));
    }

    #[test]
    fn retained_tps_shows_zero_rpm_after_the_window() {
        let mut tp = t();
        let now = base();
        tp.start("a", true, now);
        tp.add_chars(8, now);
        tp.finish(
            "a",
            Some(12),
            Some(1_000),
            now + Duration::from_millis(1_000),
        );
        let later = now + RPM_WINDOW + Duration::from_millis(1);
        let labels = tp.labels(later, 10.0);
        assert_eq!(labels.tps.unwrap().text, "12 tps");
        assert_eq!(labels.rpm.as_deref(), Some("0 rpm"));
    }

    #[test]
    fn tokenless_call_hides_after_the_window() {
        let mut tp = t();
        let now = base();
        tp.start("a", true, now);
        tp.finish("a", Some(0), None, now + Duration::from_millis(10));
        assert_eq!(tp.labels(now, 10.0).rpm.as_deref(), Some("1 rpm"));
        assert!(tp.labels(now, 10.0).tps.is_none());
        let later = now + RPM_WINDOW + Duration::from_millis(1);
        assert!(tp.labels(later, 10.0).is_empty());
    }

    #[test]
    fn failed_call_with_text_keeps_the_estimate() {
        let mut tp = t();
        let now = base();
        tp.start("a", true, now);
        tp.add_chars(4, now);
        tp.finish("a", None, Some(500), now + Duration::from_millis(500));
        assert_eq!(tp.labels(now, 10.0).tps.unwrap().text, "2.0 tps");
    }

    #[test]
    fn next_user_call_hides_the_previous_tps_until_its_first_token() {
        let mut tp = t();
        let now = base();
        tp.start("a", true, now);
        tp.finish("a", Some(20), Some(1_000), now);
        tp.start("b", true, now);
        let labels = tp.labels(now, 10.0);
        assert!(labels.tps.is_none());
        assert_eq!(labels.rpm.as_deref(), Some("2 rpm"));
    }

    #[test]
    fn retry_of_the_same_id_counts_again_and_clears_tps() {
        let mut tp = t();
        let now = base();
        tp.start("a", true, now);
        tp.add_chars(40, now);
        tp.start("a", true, now + Duration::from_millis(10));
        let labels = tp.labels(now + Duration::from_millis(10), 10.0);
        assert!(labels.tps.is_none());
        assert_eq!(labels.rpm.as_deref(), Some("2 rpm"));
    }

    #[test]
    fn background_call_does_not_steal_tps_from_a_user_call() {
        let mut tp = t();
        let now = base();
        tp.start("user", true, now);
        tp.add_chars(40, now);
        tp.start("compact", false, now);
        tp.finish(
            "compact",
            Some(100),
            Some(1_000),
            now + Duration::from_millis(50),
        );
        let labels = tp.labels(now + Duration::from_millis(1_000), 10.0);
        assert_eq!(labels.tps.unwrap().text, "10 tps");
        assert_eq!(labels.rpm.as_deref(), Some("2 rpm"));
    }

    #[test]
    fn background_call_owns_tps_when_the_user_call_is_idle() {
        let mut tp = t();
        let now = base();
        tp.start("compact", false, now);
        tp.finish("compact", Some(8), Some(1_000), now);
        let labels = tp.labels(now, 10.0);
        let tps = labels.tps.expect("tps");
        assert_eq!(tps.text, "8.0 tps");
        assert!(tps.warn);
    }

    #[test]
    fn background_waiting_on_a_user_call_takes_over_when_the_user_call_ends() {
        let mut tp = t();
        let now = base();
        tp.start("user", true, now);
        tp.start("compact", false, now);
        tp.finish("user", None, None, now);
        // Compact has not produced a token, so the retained user speed stays hidden.
        assert!(tp.labels(now, 10.0).tps.is_none());
        tp.finish("compact", Some(4), Some(1_000), now);
        assert_eq!(tp.labels(now, 10.0).tps.unwrap().text, "4.0 tps");
    }

    #[test]
    fn warning_uses_the_drawn_number() {
        let (text, shown) = format_tps(9.96);
        assert_eq!(text, "10 tps");
        assert_eq!(shown, 10.0);
        assert!(!(shown < 10.0));
        let (text, shown) = format_tps(8.36);
        assert_eq!(text, "8.4 tps");
        assert_eq!(shown, 8.4);
        let (text, shown) = format_tps(10.5);
        assert_eq!(text, "11 tps");
        assert_eq!(shown, 11.0);
    }

    #[test]
    fn zero_threshold_disables_the_warning() {
        let mut tp = t();
        let now = base();
        tp.start("a", true, now);
        tp.finish("a", Some(1), Some(1_000), now);
        assert!(!tp.labels(now, 0.0).tps.unwrap().warn);
    }

    #[test]
    fn resolve_warn_tps_keeps_zero_and_replaces_garbage_with_ten() {
        assert_eq!(resolve_warn_tps(None), 10.0);
        assert_eq!(resolve_warn_tps(Some(0.0)), 0.0);
        assert_eq!(resolve_warn_tps(Some(8.5)), 8.5);
        assert_eq!(resolve_warn_tps(Some(-1.0)), 10.0);
        assert_eq!(resolve_warn_tps(Some(f64::NAN)), 10.0);
    }
}
