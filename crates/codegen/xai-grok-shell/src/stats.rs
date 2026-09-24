//! Append-only log of model calls for `/stats`.
//!
//! One JSON object per line under `$GROK_HOME/stats/calls.jsonl`. Lines carry
//! tokens, timing, and cost. They do not carry prompt or response text.
//! Reads skip a bad line and keep the rest.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use chrono::{DateTime, Datelike, Duration, Local, TimeZone, Timelike};
use serde::{Deserialize, Serialize};

const MAX_READ_BYTES: u64 = 64 * 1024 * 1024;
const MAX_REQUEST_ROWS: usize = 1_200;
const MAX_ERROR_ROWS: usize = 400;
const MAX_BUCKETS: usize = 400;
const SPARK_POINTS: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeRange {
    All,
    Today,
    Days7,
    Days30,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum StatEvent {
    Call {
        ts_ms: i64,
        session_id: String,
        model: String,
        request_id: String,
        ok: bool,
        input_tokens: u64,
        output_tokens: u64,
        cache_read_tokens: u64,
        reasoning_tokens: u64,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        ttft_ms: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        decode_ms: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        duration_ms: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cost_usd_ticks: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        error_kind: Option<String>,
    },
    Retry {
        ts_ms: i64,
        session_id: String,
        request_id: String,
        kind: String,
    },
}

/// One finished model call, stamped with the wall clock when it is appended.
#[derive(Debug, Clone)]
pub struct CallStat {
    pub session_id: String,
    pub model: String,
    pub request_id: String,
    pub ok: bool,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub reasoning_tokens: u64,
    pub ttft_ms: Option<u64>,
    pub decode_ms: Option<u64>,
    pub duration_ms: Option<u64>,
    pub cost_usd_ticks: Option<i64>,
    pub error_kind: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Summary {
    pub requests: u64,
    pub failures: u64,
    pub retries: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub reasoning_tokens: u64,
    /// `None` when no call in range reported a cost.
    pub cost_usd_ticks: Option<i64>,
    pub cost_partial: bool,
    pub ttft_avg_ms: Option<f64>,
    pub ttft_p50_ms: Option<u64>,
    pub ttft_p95_ms: Option<u64>,
    pub tps_avg: Option<f64>,
    pub tps_p50: Option<f64>,
    pub tps_p95: Option<f64>,
    pub duration_avg_ms: Option<f64>,
    /// Completed calls / (completed + failed). `None` when both are zero.
    pub success_rate: Option<f64>,
    /// Cache reads / prompt tokens. Prompt tokens already include the cache read.
    pub cache_hit_rate: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Bucket {
    pub start_ms: i64,
    pub requests: u64,
    pub failures: u64,
    pub input_tokens: u64,
    pub cache_read_tokens: u64,
    pub output_tokens: u64,
    pub ttft_avg_ms: Option<f64>,
    pub tps_avg: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GroupRow {
    pub key: String,
    pub label: String,
    pub requests: u64,
    pub failures: u64,
    pub retries: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub ttft_p50_ms: Option<u64>,
    pub ttft_p95_ms: Option<u64>,
    pub tps_avg: Option<f64>,
    pub cost_usd_ticks: Option<i64>,
    pub cost_partial: bool,
    pub last_ts_ms: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RequestRow {
    pub ts_ms: i64,
    pub session_id: String,
    pub session_label: String,
    pub model: String,
    pub ok: bool,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
    pub ttft_ms: Option<u64>,
    pub tps: Option<f64>,
    pub duration_ms: Option<u64>,
    pub cost_usd_ticks: Option<i64>,
    pub error_kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorEvent {
    pub ts_ms: i64,
    pub session_id: String,
    pub session_label: String,
    pub kind: String,
    pub failed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReasonCount {
    pub kind: String,
    pub count: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Spark {
    pub requests: Vec<u64>,
    pub tokens: Vec<u64>,
    pub ttft_ms: Vec<u64>,
    pub tps: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Report {
    pub summary: Summary,
    pub hourly: Vec<Bucket>,
    pub daily: Vec<Bucket>,
    pub models: Vec<GroupRow>,
    pub sessions: Vec<GroupRow>,
    pub requests: Vec<RequestRow>,
    pub reasons: Vec<ReasonCount>,
    pub errors: Vec<ErrorEvent>,
    pub spark: Spark,
}

pub fn calls_path() -> PathBuf {
    crate::util::grok_home::grok_home()
        .join("stats")
        .join("calls.jsonl")
}

pub fn append_call(stat: CallStat) {
    append_event(
        &calls_path(),
        &StatEvent::Call {
            ts_ms: unix_now_ms(),
            session_id: stat.session_id,
            model: stat.model,
            request_id: stat.request_id,
            ok: stat.ok,
            input_tokens: stat.input_tokens,
            output_tokens: stat.output_tokens,
            cache_read_tokens: stat.cache_read_tokens,
            reasoning_tokens: stat.reasoning_tokens,
            ttft_ms: stat.ttft_ms,
            decode_ms: stat.decode_ms,
            duration_ms: stat.duration_ms,
            cost_usd_ticks: stat.cost_usd_ticks,
            error_kind: stat.error_kind,
        },
    );
}

pub fn append_retry(session_id: &str, request_id: &str, kind: &str) {
    append_event(
        &calls_path(),
        &StatEvent::Retry {
            ts_ms: unix_now_ms(),
            session_id: session_id.to_string(),
            request_id: request_id.to_string(),
            kind: kind.to_string(),
        },
    );
}

pub fn load_events(path: &Path) -> Vec<StatEvent> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return Vec::new(),
    };
    let len = file.metadata().ok().map(|meta| meta.len()).unwrap_or(0);
    let reader: Box<dyn BufRead> = if len > MAX_READ_BYTES {
        let start = len - MAX_READ_BYTES;
        match file.try_clone().and_then(|cloned| {
            use std::io::{Seek, SeekFrom};
            let mut cloned = cloned;
            cloned.seek(SeekFrom::Start(start))?;
            Ok(cloned)
        }) {
            Ok(cloned) => Box::new(BufReader::new(cloned)),
            Err(_) => Box::new(BufReader::new(file)),
        }
    } else {
        Box::new(BufReader::new(file))
    };
    let mut events = Vec::new();
    let mut skipped_partial = len > MAX_READ_BYTES;
    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if skipped_partial {
            skipped_partial = false;
            continue;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if let Ok(event) = serde_json::from_str::<StatEvent>(trimmed) {
            events.push(event);
        }
    }
    events
}

pub fn load_session_titles(sessions_root: &Path) -> HashMap<String, String> {
    let mut titles = HashMap::new();
    let Ok(cwds) = std::fs::read_dir(sessions_root) else {
        return titles;
    };
    for cwd in cwds.flatten() {
        let Ok(file_type) = cwd.file_type() else {
            continue;
        };
        if !file_type.is_dir() {
            continue;
        }
        let Ok(sessions) = std::fs::read_dir(cwd.path()) else {
            continue;
        };
        for session in sessions.flatten() {
            let summary = session.path().join("summary.json");
            if !summary.is_file() {
                continue;
            }
            let Ok(bytes) = std::fs::read(&summary) else {
                continue;
            };
            let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                continue;
            };
            let title = value
                .get("generated_title")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    value
                        .get("session_summary")
                        .and_then(|v| v.as_str())
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                });
            if let Some(title) = title {
                titles.insert(
                    session.file_name().to_string_lossy().into_owned(),
                    title.to_string(),
                );
            }
        }
    }
    titles
}

pub fn build_report(
    events: &[StatEvent],
    range: TimeRange,
    now_ms: i64,
    titles: &HashMap<String, String>,
) -> Report {
    let start = range_start_ms(range, now_ms);
    let mut calls: Vec<CallView> = Vec::new();
    let mut retries: Vec<RetryView> = Vec::new();
    for event in events {
        match event {
            StatEvent::Call {
                ts_ms,
                session_id,
                model,
                request_id,
                ok,
                input_tokens,
                output_tokens,
                cache_read_tokens,
                reasoning_tokens,
                ttft_ms,
                decode_ms,
                duration_ms,
                cost_usd_ticks,
                error_kind,
            } => {
                if !in_range(*ts_ms, start, now_ms) {
                    continue;
                }
                calls.push(CallView {
                    ts_ms: *ts_ms,
                    session_id: session_id.clone(),
                    model: if model.is_empty() {
                        "unknown".to_string()
                    } else {
                        model.clone()
                    },
                    request_id: request_id.clone(),
                    ok: *ok,
                    input_tokens: *input_tokens,
                    output_tokens: *output_tokens,
                    cache_read_tokens: *cache_read_tokens,
                    reasoning_tokens: *reasoning_tokens,
                    ttft_ms: *ttft_ms,
                    decode_ms: *decode_ms,
                    duration_ms: *duration_ms,
                    cost_usd_ticks: *cost_usd_ticks,
                    error_kind: error_kind.clone(),
                });
            }
            StatEvent::Retry {
                ts_ms,
                session_id,
                request_id,
                kind,
            } => {
                if in_range(*ts_ms, start, now_ms) {
                    retries.push(RetryView {
                        ts_ms: *ts_ms,
                        session_id: session_id.clone(),
                        request_id: request_id.clone(),
                        kind: kind.clone(),
                    });
                }
            }
        }
    }

    let mut call_index: HashMap<&str, &CallView> = HashMap::new();
    for call in &calls {
        call_index.insert(call.request_id.as_str(), call);
    }

    let summary = summarize(&calls, &retries);
    let hourly = buckets(&calls, false, start, now_ms);
    let daily = buckets(&calls, true, start, now_ms);
    let models = group_calls(&calls, &retries, &call_index, titles, true);
    let sessions = group_calls(&calls, &retries, &call_index, titles, false);
    let requests = request_rows(&calls, titles);
    let (reasons, errors) = error_report(&calls, &retries, titles);
    let spark = spark_from(&hourly);

    Report {
        summary,
        hourly,
        daily,
        models,
        sessions,
        requests,
        reasons,
        errors,
        spark,
    }
}

pub fn format_tokens(n: u64) -> String {
    const K: f64 = 1_000.0;
    const M: f64 = 1_000_000.0;
    const B: f64 = 1_000_000_000.0;
    let v = n as f64;
    if v >= B {
        format!("{:.2}B", v / B)
    } else if v >= M {
        format!("{:.2}M", v / M)
    } else if v >= K {
        format!("{:.1}K", v / K)
    } else {
        n.to_string()
    }
}

pub fn format_cost(ticks: Option<i64>, partial: bool) -> String {
    let Some(ticks) = ticks else {
        return "-".to_string();
    };
    let usd = ticks as f64 / 10_000_000_000.0;
    let body = if usd.abs() >= 100.0 {
        format!("${usd:.0}")
    } else if usd.abs() >= 1.0 {
        format!("${usd:.2}")
    } else {
        format!("${usd:.4}")
    };
    if partial { format!("{body}+") } else { body }
}

pub fn format_ms(ms: Option<u64>) -> String {
    let Some(ms) = ms else {
        return "-".to_string();
    };
    format_secs(ms as f64 / 1000.0)
}

pub fn format_secs(secs: f64) -> String {
    if !secs.is_finite() || secs < 0.0 {
        return "-".to_string();
    }
    if secs < 10.0 {
        format!("{secs:.1}s")
    } else if secs < 90.0 {
        format!("{:.0}s", secs.round())
    } else {
        format!("{:.1}m", secs / 60.0)
    }
}

pub fn format_tps(rate: Option<f64>) -> String {
    let Some(rate) = rate else {
        return "-".to_string();
    };
    if !rate.is_finite() || rate < 0.0 {
        return "-".to_string();
    }
    if rate >= 100.0 {
        format!("{:.0}", rate.round())
    } else {
        format!("{rate:.1}")
    }
}

pub fn format_pct(rate: Option<f64>) -> String {
    match rate {
        Some(rate) if rate.is_finite() => format!("{:.1}%", rate * 100.0),
        _ => "-".to_string(),
    }
}

struct CallView {
    ts_ms: i64,
    session_id: String,
    model: String,
    request_id: String,
    ok: bool,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    reasoning_tokens: u64,
    ttft_ms: Option<u64>,
    decode_ms: Option<u64>,
    duration_ms: Option<u64>,
    cost_usd_ticks: Option<i64>,
    error_kind: Option<String>,
}

struct RetryView {
    ts_ms: i64,
    session_id: String,
    request_id: String,
    kind: String,
}

#[derive(Default)]
struct Acc {
    requests: u64,
    failures: u64,
    input_tokens: u64,
    output_tokens: u64,
    cache_read_tokens: u64,
    reasoning_tokens: u64,
    cost_usd_ticks: i64,
    cost_samples: u64,
    cost_missing: u64,
    ttft: Vec<u64>,
    tps: Vec<f64>,
    duration_ms: Vec<u64>,
    last_ts_ms: i64,
}

impl Acc {
    fn add_call(&mut self, call: &CallView) {
        if call.ok {
            self.requests = self.requests.saturating_add(1);
        } else {
            self.failures = self.failures.saturating_add(1);
        }
        self.input_tokens = self.input_tokens.saturating_add(call.input_tokens);
        self.output_tokens = self.output_tokens.saturating_add(call.output_tokens);
        self.cache_read_tokens = self
            .cache_read_tokens
            .saturating_add(call.cache_read_tokens);
        self.reasoning_tokens = self.reasoning_tokens.saturating_add(call.reasoning_tokens);
        match call.cost_usd_ticks {
            Some(ticks) => {
                self.cost_usd_ticks = self.cost_usd_ticks.saturating_add(ticks);
                self.cost_samples = self.cost_samples.saturating_add(1);
            }
            None => self.cost_missing = self.cost_missing.saturating_add(1),
        }
        if let Some(ttft) = call.ttft_ms {
            self.ttft.push(ttft);
        }
        if let Some(tps) = call_tps(call) {
            self.tps.push(tps);
        }
        if let Some(duration) = call.duration_ms {
            self.duration_ms.push(duration);
        }
        if call.ts_ms > self.last_ts_ms {
            self.last_ts_ms = call.ts_ms;
        }
    }
}

fn summarize(calls: &[CallView], retries: &[RetryView]) -> Summary {
    let mut acc = Acc::default();
    for call in calls {
        acc.add_call(call);
    }
    let mut summary = summary_from_acc(&acc);
    summary.retries = retries.len() as u64;
    summary
}

fn summary_from_acc(acc: &Acc) -> Summary {
    let total = acc.requests.saturating_add(acc.failures);
    Summary {
        requests: acc.requests,
        failures: acc.failures,
        retries: 0,
        input_tokens: acc.input_tokens,
        output_tokens: acc.output_tokens,
        cache_read_tokens: acc.cache_read_tokens,
        reasoning_tokens: acc.reasoning_tokens,
        cost_usd_ticks: (acc.cost_samples > 0).then_some(acc.cost_usd_ticks),
        cost_partial: acc.cost_samples > 0 && acc.cost_missing > 0,
        ttft_avg_ms: avg_u64(&acc.ttft),
        ttft_p50_ms: percentile_u64(&acc.ttft, 0.50),
        ttft_p95_ms: percentile_u64(&acc.ttft, 0.95),
        tps_avg: avg_f64(&acc.tps),
        tps_p50: percentile_f64(&acc.tps, 0.50),
        tps_p95: percentile_f64(&acc.tps, 0.95),
        duration_avg_ms: avg_u64(&acc.duration_ms),
        success_rate: (total > 0).then(|| acc.requests as f64 / total as f64),
        cache_hit_rate: (acc.input_tokens > 0)
            .then(|| acc.cache_read_tokens as f64 / acc.input_tokens as f64),
    }
}

fn group_calls(
    calls: &[CallView],
    retries: &[RetryView],
    call_index: &HashMap<&str, &CallView>,
    titles: &HashMap<String, String>,
    by_model: bool,
) -> Vec<GroupRow> {
    let mut groups: HashMap<String, Acc> = HashMap::new();
    let mut retry_counts: HashMap<String, u64> = HashMap::new();
    for call in calls {
        let key = if by_model {
            call.model.clone()
        } else {
            call.session_id.clone()
        };
        groups.entry(key).or_default().add_call(call);
    }
    for retry in retries {
        let key = if by_model {
            call_index
                .get(retry.request_id.as_str())
                .map(|call| call.model.clone())
        } else {
            Some(retry.session_id.clone())
        };
        if let Some(key) = key {
            *retry_counts.entry(key).or_default() += 1;
        }
    }
    let mut rows = Vec::new();
    for (key, acc) in groups {
        let summary = summary_from_acc(&acc);
        let label = if by_model {
            key.clone()
        } else {
            titles.get(&key).cloned().unwrap_or_else(|| short_id(&key))
        };
        let retries = retry_counts.get(&key).copied().unwrap_or(0);
        rows.push(GroupRow {
            key,
            label,
            requests: summary.requests,
            failures: summary.failures,
            retries,
            input_tokens: summary.input_tokens,
            output_tokens: summary.output_tokens,
            cache_read_tokens: summary.cache_read_tokens,
            ttft_p50_ms: summary.ttft_p50_ms,
            ttft_p95_ms: summary.ttft_p95_ms,
            tps_avg: summary.tps_avg,
            cost_usd_ticks: summary.cost_usd_ticks,
            cost_partial: summary.cost_partial,
            last_ts_ms: acc.last_ts_ms,
        });
    }
    rows.sort_by(|a, b| b.requests.cmp(&a.requests).then(a.label.cmp(&b.label)));
    rows
}

fn request_rows(calls: &[CallView], titles: &HashMap<String, String>) -> Vec<RequestRow> {
    let mut rows: Vec<RequestRow> = calls
        .iter()
        .map(|call| RequestRow {
            ts_ms: call.ts_ms,
            session_label: titles
                .get(&call.session_id)
                .cloned()
                .unwrap_or_else(|| short_id(&call.session_id)),
            session_id: call.session_id.clone(),
            model: call.model.clone(),
            ok: call.ok,
            input_tokens: call.input_tokens,
            output_tokens: call.output_tokens,
            cache_read_tokens: call.cache_read_tokens,
            ttft_ms: call.ttft_ms,
            tps: call_tps(call),
            duration_ms: call.duration_ms,
            cost_usd_ticks: call.cost_usd_ticks,
            error_kind: call.error_kind.clone(),
        })
        .collect();
    rows.sort_by(|a, b| b.ts_ms.cmp(&a.ts_ms));
    if rows.len() > MAX_REQUEST_ROWS {
        rows.truncate(MAX_REQUEST_ROWS);
    }
    rows
}

fn error_report(
    calls: &[CallView],
    retries: &[RetryView],
    titles: &HashMap<String, String>,
) -> (Vec<ReasonCount>, Vec<ErrorEvent>) {
    let mut counts: HashMap<String, u64> = HashMap::new();
    let mut events = Vec::new();
    for retry in retries {
        *counts.entry(retry.kind.clone()).or_default() += 1;
        events.push(ErrorEvent {
            ts_ms: retry.ts_ms,
            session_label: titles
                .get(&retry.session_id)
                .cloned()
                .unwrap_or_else(|| short_id(&retry.session_id)),
            session_id: retry.session_id.clone(),
            kind: retry.kind.clone(),
            failed: false,
        });
    }
    for call in calls {
        if call.ok {
            continue;
        }
        let kind = call
            .error_kind
            .clone()
            .filter(|kind| !kind.is_empty())
            .unwrap_or_else(|| "failed".to_string());
        *counts.entry(kind.clone()).or_default() += 1;
        events.push(ErrorEvent {
            ts_ms: call.ts_ms,
            session_label: titles
                .get(&call.session_id)
                .cloned()
                .unwrap_or_else(|| short_id(&call.session_id)),
            session_id: call.session_id.clone(),
            kind,
            failed: true,
        });
    }
    let mut reasons: Vec<ReasonCount> = counts
        .into_iter()
        .map(|(kind, count)| ReasonCount { kind, count })
        .collect();
    reasons.sort_by(|a, b| b.count.cmp(&a.count).then(a.kind.cmp(&b.kind)));
    events.sort_by(|a, b| b.ts_ms.cmp(&a.ts_ms));
    if events.len() > MAX_ERROR_ROWS {
        events.truncate(MAX_ERROR_ROWS);
    }
    (reasons, events)
}

fn buckets(calls: &[CallView], daily: bool, start: Option<i64>, now_ms: i64) -> Vec<Bucket> {
    if calls.is_empty() {
        return Vec::new();
    }
    let mut by_start: HashMap<i64, Acc> = HashMap::new();
    let mut earliest = now_ms;
    for call in calls {
        let key = align_local(call.ts_ms, daily);
        if call.ts_ms < earliest {
            earliest = call.ts_ms;
        }
        by_start.entry(key).or_default().add_call(call);
    }
    let from = start.unwrap_or(earliest);
    let mut cursor = local_dt(align_local(from, daily));
    let end = local_dt(align_local(now_ms, daily));
    let mut out = Vec::new();
    let mut guard = 0;
    while cursor <= end && guard < MAX_BUCKETS + 8 {
        guard += 1;
        let key = cursor.timestamp_millis();
        let acc = by_start.get(&key);
        out.push(bucket_of(key, acc));
        cursor = if daily {
            cursor + Duration::days(1)
        } else {
            cursor + Duration::hours(1)
        };
    }
    if out.len() > MAX_BUCKETS {
        let skip = out.len() - MAX_BUCKETS;
        out.drain(0..skip);
    }
    out
}

fn bucket_of(start_ms: i64, acc: Option<&Acc>) -> Bucket {
    let Some(acc) = acc else {
        return Bucket {
            start_ms,
            requests: 0,
            failures: 0,
            input_tokens: 0,
            cache_read_tokens: 0,
            output_tokens: 0,
            ttft_avg_ms: None,
            tps_avg: None,
        };
    };
    Bucket {
        start_ms,
        requests: acc.requests,
        failures: acc.failures,
        input_tokens: acc.input_tokens,
        cache_read_tokens: acc.cache_read_tokens,
        output_tokens: acc.output_tokens,
        ttft_avg_ms: avg_u64(&acc.ttft),
        tps_avg: avg_f64(&acc.tps),
    }
}

fn spark_from(hourly: &[Bucket]) -> Spark {
    let slice = if hourly.len() > SPARK_POINTS {
        let start = hourly.len() - SPARK_POINTS;
        hourly.get(start..).unwrap_or(hourly)
    } else {
        hourly
    };
    Spark {
        requests: slice.iter().map(|bucket| bucket.requests).collect(),
        tokens: slice
            .iter()
            .map(|bucket| bucket.input_tokens.saturating_add(bucket.output_tokens))
            .collect(),
        ttft_ms: slice
            .iter()
            .map(|bucket| bucket.ttft_avg_ms.unwrap_or(0.0).round().max(0.0) as u64)
            .collect(),
        tps: slice
            .iter()
            .map(|bucket| bucket.tps_avg.unwrap_or(0.0).round().max(0.0) as u64)
            .collect(),
    }
}

fn call_tps(call: &CallView) -> Option<f64> {
    let ms = call.decode_ms?;
    if ms == 0 || call.output_tokens == 0 {
        return None;
    }
    Some(call.output_tokens as f64 / (ms as f64 / 1000.0))
}

fn in_range(ts_ms: i64, start: Option<i64>, now_ms: i64) -> bool {
    if ts_ms > now_ms.saturating_add(60_000) {
        return false;
    }
    start.is_none_or(|start| ts_ms >= start)
}

fn range_start_ms(range: TimeRange, now_ms: i64) -> Option<i64> {
    match range {
        TimeRange::All => None,
        TimeRange::Today => Some(align_local(now_ms, true)),
        TimeRange::Days7 => Some(now_ms.saturating_sub(7 * 86_400_000)),
        TimeRange::Days30 => Some(now_ms.saturating_sub(30 * 86_400_000)),
    }
}

fn align_local(ts_ms: i64, daily: bool) -> i64 {
    let dt = local_dt(ts_ms);
    let naive = if daily {
        dt.date_naive().and_hms_opt(0, 0, 0)
    } else {
        dt.date_naive().and_hms_opt(dt.hour(), 0, 0)
    };
    naive
        .and_then(|naive| Local.from_local_datetime(&naive).single())
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(ts_ms)
}

fn local_dt(ts_ms: i64) -> DateTime<Local> {
    DateTime::from_timestamp_millis(ts_ms)
        .unwrap_or_else(chrono::Utc::now)
        .with_timezone(&Local)
}

fn avg_u64(values: &[u64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let sum: u128 = values.iter().map(|v| u128::from(*v)).sum();
    Some(sum as f64 / values.len() as f64)
}

fn avg_f64(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    Some(values.iter().sum::<f64>() / values.len() as f64)
}

fn percentile_u64(values: &[u64], p: f64) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let idx = percentile_index(sorted.len(), p);
    sorted.get(idx).copied()
}

fn percentile_f64(values: &[f64], p: f64) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if sorted.is_empty() {
        return None;
    }
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = percentile_index(sorted.len(), p);
    sorted.get(idx).copied()
}

fn percentile_index(len: usize, p: f64) -> usize {
    if len == 0 {
        return 0;
    }
    let rank = ((len - 1) as f64 * p).round() as usize;
    rank.min(len - 1)
}

fn short_id(id: &str) -> String {
    let mut chars = id.chars();
    let mut out = String::new();
    for _ in 0..8 {
        match chars.next() {
            Some(ch) => out.push(ch),
            None => break,
        }
    }
    if out.is_empty() { id.to_string() } else { out }
}

fn unix_now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn append_event(path: &Path, event: &StatEvent) {
    let Ok(line) = serde_json::to_string(event) else {
        return;
    };
    if let Some(parent) = path.parent()
        && let Err(err) = std::fs::create_dir_all(parent)
    {
        tracing::warn!(error = %err, "stats dir");
        return;
    }
    let mut file = match OpenOptions::new().create(true).append(true).open(path) {
        Ok(file) => file,
        Err(err) => {
            tracing::warn!(error = %err, "stats log");
            return;
        }
    };
    if let Err(err) = writeln!(file, "{line}") {
        tracing::warn!(error = %err, "stats append");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(ts: i64, ok: bool, output: u64, ttft: u64, decode: u64) -> StatEvent {
        StatEvent::Call {
            ts_ms: ts,
            session_id: "sess-1".into(),
            model: "grok-4".into(),
            request_id: format!("r{ts}"),
            ok,
            input_tokens: 100,
            output_tokens: output,
            cache_read_tokens: 80,
            reasoning_tokens: 0,
            ttft_ms: Some(ttft),
            decode_ms: Some(decode),
            duration_ms: Some(ttft + decode),
            cost_usd_ticks: Some(50_000_000),
            error_kind: if ok { None } else { Some("http".into()) },
        }
    }

    #[test]
    fn summary_matches_tokstat_ratios() {
        let now = 1_700_000_000_000;
        let events = vec![
            call(now - 1_000, true, 50, 2_000, 1_000),
            call(now - 500, false, 0, 9_000, 0),
            StatEvent::Retry {
                ts_ms: now - 800,
                session_id: "sess-1".into(),
                request_id: format!("r{}", now - 1_000),
                kind: "rate_limited".into(),
            },
        ];
        let report = build_report(&events, TimeRange::All, now, &HashMap::new());
        assert_eq!(report.summary.requests, 1);
        assert_eq!(report.summary.failures, 1);
        assert_eq!(report.summary.retries, 1);
        assert_eq!(report.summary.success_rate, Some(0.5));
        assert_eq!(report.summary.cache_hit_rate, Some(0.8));
        // 50 tokens over 1.0s of decode. Both TTFT samples sit in the average.
        assert_eq!(report.summary.tps_avg, Some(50.0));
        assert_eq!(report.summary.ttft_avg_ms, Some(5_500.0));
        assert_eq!(report.requests.len(), 2);
        assert_eq!(report.models.first().map(|row| row.retries), Some(1));
        assert_eq!(
            report.reasons.first().map(|reason| reason.kind.as_str()),
            Some("http")
        );
    }

    #[test]
    fn today_drops_yesterdays_call() {
        let now = unix_now_ms();
        let yesterday = now - 2 * 86_400_000;
        let events = vec![
            call(yesterday, true, 10, 100, 100),
            call(now - 1_000, true, 10, 100, 100),
        ];
        let report = build_report(&events, TimeRange::Today, now, &HashMap::new());
        assert_eq!(report.summary.requests, 1);
    }

    #[test]
    fn missing_cost_renders_as_dash_and_partial_marks_a_mix() {
        assert_eq!(format_cost(None, false), "-");
        assert_eq!(format_cost(Some(12_400_000_000), false), "$1.24");
        assert_eq!(format_cost(Some(12_400_000_000), true), "$1.24+");
    }

    #[test]
    fn append_round_trips_one_line() {
        let dir = std::env::temp_dir().join(format!("grok-stats-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("calls.jsonl");
        append_event(
            &path,
            &StatEvent::Retry {
                ts_ms: 5,
                session_id: "s".into(),
                request_id: "r".into(),
                kind: "http".into(),
            },
        );
        let mut file = OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(file, "not-json").unwrap();
        let events = load_events(&path);
        assert_eq!(events.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn session_title_prefers_generated_title() {
        let dir = std::env::temp_dir().join(format!("grok-titles-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let session = dir.join("cwd").join("abc123");
        std::fs::create_dir_all(&session).unwrap();
        std::fs::write(
            session.join("summary.json"),
            br#"{"generated_title":"Fix pager","session_summary":"older"}"#,
        )
        .unwrap();
        let titles = load_session_titles(&dir);
        assert_eq!(titles.get("abc123").map(String::as_str), Some("Fix pager"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
