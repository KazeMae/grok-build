//! `/stats`: local usage and speed, across every session on this machine.
//!
//! Overview, trends, models, sessions, requests, and errors. Numbers come from
//! the append-only call log. Sessions from before that log existed stay empty here.

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEventKind};
use ratatui::buffer::Buffer;
use ratatui::layout::{Position, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Sparkline, Widget};
use unicode_width::UnicodeWidthStr;

use crate::theme::Theme;
use crate::views::modal_window::{self as mw, ModalSizing, Shortcut};

use xai_grok_shell::stats::{
    self, GroupRow, Report, RequestRow, TimeRange, format_cost, format_ms, format_pct, format_secs,
    format_tokens, format_tps,
};

pub const TAB_LABELS: [&str; 6] = [
    "Overview", "Trends", "Models", "Sessions", "Requests", "Errors",
];

#[derive(Debug)]
pub struct StatsModalState {
    pub window: mw::ModalWindowState,
    tab: usize,
    range: TimeRange,
    daily: bool,
    paused: bool,
    filter: String,
    filtering: bool,
    cursor: usize,
    scroll: usize,
    sort_col: usize,
    sort_desc: bool,
    report: Report,
    loaded_at: Instant,
    header_hits: Vec<(Rect, usize)>,
}

pub enum StatsOutcome {
    Close,
    Changed,
    Unchanged,
}

impl StatsModalState {
    pub fn open() -> Self {
        let mut state = Self {
            window: mw::ModalWindowState::with_tabs(TAB_LABELS.len()),
            tab: 0,
            range: TimeRange::All,
            daily: false,
            paused: false,
            filter: String::new(),
            filtering: false,
            cursor: 0,
            scroll: 0,
            sort_col: 1,
            sort_desc: true,
            report: stats::build_report(&[], TimeRange::All, 0, &Default::default()),
            loaded_at: Instant::now(),
            header_hits: Vec::new(),
        };
        state.reload();
        state
    }

    pub fn reload(&mut self) {
        let events = stats::load_events(&stats::calls_path());
        let titles = stats::load_session_titles(
            &xai_grok_shell::util::grok_home::grok_home().join("sessions"),
        );
        let now = now_ms();
        self.report = stats::build_report(&events, self.range, now, &titles);
        self.loaded_at = Instant::now();
    }

    fn refresh_if_due(&mut self) {
        if self.paused {
            return;
        }
        if self.loaded_at.elapsed() >= std::time::Duration::from_secs(2) {
            self.reload();
        }
    }

    fn set_tab(&mut self, tab: usize) {
        if self.tab == tab {
            return;
        }
        self.tab = tab;
        self.window.active_tab = tab;
        self.cursor = 0;
        self.scroll = 0;
        self.sort_col = if tab == 4 { 0 } else { 1 };
        self.sort_desc = tab != 0;
    }

    fn set_range(&mut self, range: TimeRange) {
        if self.range == range {
            return;
        }
        self.range = range;
        self.cursor = 0;
        self.scroll = 0;
        self.reload();
    }
}

pub fn render_stats_modal(
    buf: &mut Buffer,
    area: Rect,
    state: &mut StatsModalState,
    theme: &Theme,
) {
    state.refresh_if_due();
    state.header_hits.clear();
    let footer = footer();
    let config = mw::ModalWindowConfig {
        title: "Stats",
        tabs: Some(&TAB_LABELS),
        shortcuts: &footer,
        sizing: ModalSizing {
            width_pct: 0.96,
            max_width: 220,
            min_width: 68,
            v_margin: 1,
            h_pad: 1,
            v_pad: 0,
            footer_lines: 1,
        },
        fold_info: None,
    };
    let Some(content) = mw::render_modal_window(buf, area, &mut state.window, &config, theme)
    else {
        return;
    };
    state.tab = state
        .window
        .active_tab
        .min(TAB_LABELS.len().saturating_sub(1));
    paint(buf, content.content, state, theme);
}

pub fn handle_stats_key(state: &mut StatsModalState, key: &KeyEvent) -> StatsOutcome {
    if state.filtering {
        return match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                state.filtering = false;
                StatsOutcome::Changed
            }
            KeyCode::Backspace => {
                state.filter.pop();
                state.cursor = 0;
                state.scroll = 0;
                StatsOutcome::Changed
            }
            KeyCode::Char(ch) if key.modifiers.is_empty() => {
                state.filter.push(ch);
                state.cursor = 0;
                state.scroll = 0;
                StatsOutcome::Changed
            }
            _ => StatsOutcome::Unchanged,
        };
    }
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('r' | 'R'))
    {
        state.reload();
        return StatsOutcome::Changed;
    }
    if !key.modifiers.is_empty() {
        return StatsOutcome::Unchanged;
    }
    match key.code {
        KeyCode::Char('q') => StatsOutcome::Close,
        KeyCode::Char('o') => {
            state.set_tab(0);
            StatsOutcome::Changed
        }
        KeyCode::Char('t') => {
            state.set_tab(1);
            StatsOutcome::Changed
        }
        KeyCode::Char('m') => {
            state.set_tab(2);
            StatsOutcome::Changed
        }
        KeyCode::Char('s') => {
            state.set_tab(3);
            StatsOutcome::Changed
        }
        KeyCode::Char('r') => {
            state.set_tab(4);
            StatsOutcome::Changed
        }
        KeyCode::Char('e') => {
            state.set_tab(5);
            StatsOutcome::Changed
        }
        KeyCode::Char('1') => {
            state.set_range(TimeRange::All);
            StatsOutcome::Changed
        }
        KeyCode::Char('2') => {
            state.set_range(TimeRange::Today);
            StatsOutcome::Changed
        }
        KeyCode::Char('3') => {
            state.set_range(TimeRange::Days7);
            StatsOutcome::Changed
        }
        KeyCode::Char('4') => {
            state.set_range(TimeRange::Days30);
            StatsOutcome::Changed
        }
        KeyCode::Char('h') => {
            state.daily = false;
            StatsOutcome::Changed
        }
        KeyCode::Char('d') => {
            state.daily = true;
            StatsOutcome::Changed
        }
        KeyCode::Char('f') => {
            state.filtering = true;
            StatsOutcome::Changed
        }
        KeyCode::Char(' ') => {
            state.paused = !state.paused;
            StatsOutcome::Changed
        }
        KeyCode::Char('j') | KeyCode::Down => {
            move_cursor(state, 1);
            StatsOutcome::Changed
        }
        KeyCode::Char('k') | KeyCode::Up => {
            move_cursor(state, -1);
            StatsOutcome::Changed
        }
        KeyCode::Left => {
            state.sort_col = state.sort_col.saturating_sub(1);
            StatsOutcome::Changed
        }
        KeyCode::Right => {
            state.sort_col = state.sort_col.saturating_add(1);
            StatsOutcome::Changed
        }
        KeyCode::Enter if state.tab == 3 => open_session(state),
        _ => StatsOutcome::Unchanged,
    }
}

pub fn handle_stats_mouse(
    state: &mut StatsModalState,
    kind: MouseEventKind,
    column: u16,
    row: u16,
) -> StatsOutcome {
    match kind {
        MouseEventKind::ScrollDown => {
            move_cursor(state, 3);
            StatsOutcome::Changed
        }
        MouseEventKind::ScrollUp => {
            move_cursor(state, -3);
            StatsOutcome::Changed
        }
        MouseEventKind::Down(_) => {
            if let Some((_, col)) = state
                .header_hits
                .iter()
                .find(|(rect, _)| rect.contains(Position::new(column, row)))
            {
                let col = *col;
                if state.sort_col == col {
                    state.sort_desc = !state.sort_desc;
                } else {
                    state.sort_col = col;
                    state.sort_desc = col != 0;
                }
                return StatsOutcome::Changed;
            }
            StatsOutcome::Unchanged
        }
        _ => StatsOutcome::Unchanged,
    }
}

pub fn apply_tab(state: &mut StatsModalState, tab: usize) {
    state.set_tab(tab);
}

fn open_session(state: &mut StatsModalState) -> StatsOutcome {
    let mut rows = state.report.sessions.clone();
    sort_groups(&mut rows, state.sort_col, state.sort_desc);
    let Some(row) = filtered_groups(&rows, &state.filter)
        .into_iter()
        .nth(state.cursor)
    else {
        return StatsOutcome::Unchanged;
    };
    state.filter = row.key.clone();
    state.filtering = false;
    state.set_tab(4);
    StatsOutcome::Changed
}

fn move_cursor(state: &mut StatsModalState, delta: i32) {
    let len = row_count(state);
    if len == 0 {
        state.cursor = 0;
        return;
    }
    let next = if delta < 0 {
        state.cursor.saturating_sub(delta.unsigned_abs() as usize)
    } else {
        state.cursor.saturating_add(delta as usize).min(len - 1)
    };
    state.cursor = next;
    if state.cursor < state.scroll {
        state.scroll = state.cursor;
    }
}

fn row_count(state: &StatsModalState) -> usize {
    match state.tab {
        2 => filtered_groups(&state.report.models, &state.filter).len(),
        3 => filtered_groups(&state.report.sessions, &state.filter).len(),
        4 => filtered_requests(&state.report.requests, &state.filter).len(),
        5 => state.report.errors.len(),
        1 => {
            let buckets = if state.daily {
                &state.report.daily
            } else {
                &state.report.hourly
            };
            buckets.len()
        }
        _ => 0,
    }
}

fn paint(buf: &mut Buffer, area: Rect, state: &mut StatsModalState, theme: &Theme) {
    if area.width == 0 || area.height == 0 {
        return;
    }
    let mut y = area.y;
    let meta = format!(
        "{}   {}   {}{}",
        range_label(state.range),
        if state.daily { "day" } else { "hour" },
        if state.paused { "paused" } else { "live" },
        if state.filter.is_empty() {
            String::new()
        } else {
            format!("   filter: {}", state.filter)
        }
    );
    put(
        buf,
        area.x,
        y,
        &meta,
        area.width,
        Style::default().fg(theme.gray),
    );
    y = y.saturating_add(1);
    if y >= area.y + area.height {
        return;
    }
    let body = Rect {
        x: area.x,
        y,
        width: area.width,
        height: area.height.saturating_sub(y - area.y),
    };
    if state.report.summary.requests == 0 && state.report.summary.failures == 0 {
        put(
            buf,
            body.x,
            body.y,
            "No calls yet. The next model request shows up here.",
            body.width,
            Style::default().fg(theme.gray),
        );
        return;
    }
    match state.tab {
        0 => paint_overview(buf, body, state, theme),
        1 => paint_trends(buf, body, state, theme),
        2 => paint_groups(buf, body, state, theme, true),
        3 => paint_groups(buf, body, state, theme, false),
        4 => paint_requests(buf, body, state, theme),
        _ => paint_errors(buf, body, state, theme),
    }
}

fn paint_overview(buf: &mut Buffer, area: Rect, state: &StatsModalState, theme: &Theme) {
    let summary = &state.report.summary;
    let cards = [
        ("Requests", summary.requests.to_string()),
        (
            "Tokens",
            format_tokens(summary.input_tokens.saturating_add(summary.output_tokens)),
        ),
        (
            "TTFT",
            format!(
                "{}  p50 {}  p95 {}",
                format_ms(summary.ttft_avg_ms.map(|v| v.round() as u64)),
                format_ms(summary.ttft_p50_ms),
                format_ms(summary.ttft_p95_ms)
            ),
        ),
        (
            "TPS",
            format!(
                "{}  p50 {}  p95 {}",
                format_tps(summary.tps_avg),
                format_tps(summary.tps_p50),
                format_tps(summary.tps_p95)
            ),
        ),
        ("Success", format_pct(summary.success_rate)),
        ("Cache", format_pct(summary.cache_hit_rate)),
        (
            "Cost",
            format_cost(summary.cost_usd_ticks, summary.cost_partial),
        ),
        (
            "Duration",
            format_secs(summary.duration_avg_ms.unwrap_or(0.0) / 1000.0),
        ),
    ];
    let mut y = area.y;
    for pair in cards.chunks(2) {
        if y >= area.y + area.height {
            return;
        }
        let width = area.width / 2;
        for (i, (label, value)) in pair.iter().enumerate() {
            let x = area.x.saturating_add(width.saturating_mul(i as u16));
            put(
                buf,
                x,
                y,
                &format!("{label}  {value}"),
                width,
                Style::default().fg(theme.text_primary),
            );
        }
        y = y.saturating_add(1);
    }
    y = y.saturating_add(1);
    if y >= area.y + area.height {
        return;
    }
    let uncached = summary
        .input_tokens
        .saturating_sub(summary.cache_read_tokens);
    let mix = token_mix(uncached, summary.cache_read_tokens, summary.output_tokens);
    put(
        buf,
        area.x,
        y,
        &mix,
        area.width,
        Style::default().fg(theme.gray_bright),
    );
    y = y.saturating_add(2);
    y = spark_line(
        buf,
        area,
        y,
        "req",
        &state.report.spark.requests,
        theme.accent_user,
    );
    y = spark_line(
        buf,
        area,
        y,
        "tok",
        &state.report.spark.tokens,
        theme.accent_assistant,
    );
    y = spark_line(
        buf,
        area,
        y,
        "ttft",
        &state.report.spark.ttft_ms,
        theme.warning,
    );
    let _ = spark_line(
        buf,
        area,
        y,
        "tps",
        &state.report.spark.tps,
        theme.accent_success,
    );
}

fn paint_trends(buf: &mut Buffer, area: Rect, state: &mut StatsModalState, theme: &Theme) {
    let buckets = if state.daily {
        state.report.daily.clone()
    } else {
        state.report.hourly.clone()
    };
    if buckets.is_empty() {
        return;
    }
    let visible = area.height.saturating_sub(1) as usize;
    let start = state.scroll.min(buckets.len().saturating_sub(1));
    let mut y = area.y;
    put(
        buf,
        area.x,
        y,
        "time      req   tokens        ttft    tps",
        area.width,
        Style::default().fg(theme.gray).add_modifier(Modifier::BOLD),
    );
    y = y.saturating_add(1);
    let max_req = buckets.iter().map(|b| b.requests).max().unwrap_or(1).max(1);
    for bucket in buckets.iter().skip(start).take(visible.max(1)) {
        if y >= area.y + area.height {
            break;
        }
        let label = bucket_label(bucket.start_ms, state.daily);
        let bar_w = 8usize;
        let bar = blocks(bucket.requests as f64, max_req as f64, bar_w);
        let ttft_style = ttft_style(bucket.ttft_avg_ms, theme);
        put(
            buf,
            area.x,
            y,
            &format!(
                "{label:<8} {:>4} {bar} {:>10}  ",
                bucket.requests,
                format_tokens(bucket.input_tokens.saturating_add(bucket.output_tokens))
            ),
            area.width,
            Style::default().fg(theme.text_primary),
        );
        let ttft = format_ms(bucket.ttft_avg_ms.map(|v| v.round() as u64));
        let tps = format_tps(bucket.tps_avg);
        let x = area.x.saturating_add(28);
        put(
            buf,
            x,
            y,
            &format!("{ttft:<8}{tps}"),
            area.width.saturating_sub(28),
            ttft_style,
        );
        y = y.saturating_add(1);
    }
}

fn paint_groups(
    buf: &mut Buffer,
    area: Rect,
    state: &mut StatsModalState,
    theme: &Theme,
    models: bool,
) {
    let headers: [&str; 8] = if models {
        [
            "model", "req", "fail", "cache", "ttft", "tps", "retry", "cost",
        ]
    } else {
        [
            "session", "req", "fail", "tokens", "ttft", "tps", "retry", "cost",
        ]
    };
    let source = if models {
        state.report.models.clone()
    } else {
        state.report.sessions.clone()
    };
    let mut rows = source;
    sort_groups(&mut rows, state.sort_col, state.sort_desc);
    let rows = filtered_groups(&rows, &state.filter);
    paint_header(buf, area, state, theme, &headers);
    let body_y = area.y.saturating_add(1);
    let visible = area.height.saturating_sub(1) as usize;
    fit_scroll(state, rows.len(), visible);
    for (i, row) in rows.iter().skip(state.scroll).take(visible).enumerate() {
        let y = body_y.saturating_add(i as u16);
        if y >= area.y + area.height {
            break;
        }
        let selected = state.scroll + i == state.cursor;
        let style = if selected {
            Style::default()
                .bg(theme.bg_highlight)
                .fg(theme.text_primary)
        } else {
            Style::default().fg(theme.text_primary)
        };
        let cells = group_cells(row, models);
        put(buf, area.x, y, &join_cells(&cells), area.width, style);
    }
}

fn paint_requests(buf: &mut Buffer, area: Rect, state: &mut StatsModalState, theme: &Theme) {
    let headers = ["time", "model", "session", "in", "out", "ttft", "tps", "ok"];
    let mut rows = state.report.requests.clone();
    sort_requests(&mut rows, state.sort_col, state.sort_desc);
    let rows = filtered_requests(&rows, &state.filter);
    paint_header(buf, area, state, theme, &headers);
    let body_y = area.y.saturating_add(1);
    let visible = area.height.saturating_sub(1) as usize;
    fit_scroll(state, rows.len(), visible);
    for (i, row) in rows.iter().skip(state.scroll).take(visible).enumerate() {
        let y = body_y.saturating_add(i as u16);
        if y >= area.y + area.height {
            break;
        }
        let selected = state.scroll + i == state.cursor;
        let style = if selected {
            Style::default()
                .bg(theme.bg_highlight)
                .fg(theme.text_primary)
        } else if row.ok {
            Style::default().fg(theme.text_primary)
        } else {
            Style::default().fg(theme.accent_error)
        };
        let cells = request_cells(row);
        put(buf, area.x, y, &join_cells(&cells), area.width, style);
    }
}

fn paint_errors(buf: &mut Buffer, area: Rect, state: &mut StatsModalState, theme: &Theme) {
    let mut y = area.y;
    put(
        buf,
        area.x,
        y,
        "reason",
        area.width,
        Style::default().fg(theme.gray).add_modifier(Modifier::BOLD),
    );
    y = y.saturating_add(1);
    for reason in &state.report.reasons {
        if y + 4 >= area.y + area.height {
            break;
        }
        put(
            buf,
            area.x,
            y,
            &format!("{:<18} {}", reason.kind, reason.count),
            area.width,
            Style::default().fg(theme.text_primary),
        );
        y = y.saturating_add(1);
    }
    y = y.saturating_add(1);
    if y >= area.y + area.height {
        return;
    }
    put(
        buf,
        area.x,
        y,
        "when       kind              session",
        area.width,
        Style::default().fg(theme.gray).add_modifier(Modifier::BOLD),
    );
    y = y.saturating_add(1);
    let visible = (area.y + area.height).saturating_sub(y) as usize;
    fit_scroll(state, state.report.errors.len(), visible);
    for event in state.report.errors.iter().skip(state.scroll).take(visible) {
        if y >= area.y + area.height {
            break;
        }
        let mark = if event.failed { "fail" } else { "retry" };
        put(
            buf,
            area.x,
            y,
            &format!(
                "{:<10} {:<16} {} {mark}",
                clock(event.ts_ms),
                clip(&event.kind, 16),
                clip(&event.session_label, 24)
            ),
            area.width,
            Style::default().fg(if event.failed {
                theme.accent_error
            } else {
                theme.warning
            }),
        );
        y = y.saturating_add(1);
    }
}

fn paint_header(
    buf: &mut Buffer,
    area: Rect,
    state: &mut StatsModalState,
    theme: &Theme,
    headers: &[&str],
) {
    let mut x = area.x;
    for (i, header) in headers.iter().enumerate() {
        let width = col_width(i, headers.len(), area.width);
        if width == 0 {
            break;
        }
        let marked = if state.sort_col == i {
            if state.sort_desc {
                format!("{header}↓")
            } else {
                format!("{header}↑")
            }
        } else {
            (*header).to_string()
        };
        put(
            buf,
            x,
            area.y,
            &marked,
            width,
            Style::default().fg(theme.gray).add_modifier(Modifier::BOLD),
        );
        state.header_hits.push((
            Rect {
                x,
                y: area.y,
                width,
                height: 1,
            },
            i,
        ));
        x = x.saturating_add(width);
    }
}

fn group_cells(row: &GroupRow, models: bool) -> Vec<String> {
    let third = if models {
        format_pct(
            (row.input_tokens > 0).then(|| row.cache_read_tokens as f64 / row.input_tokens as f64),
        )
    } else {
        format_tokens(row.input_tokens.saturating_add(row.output_tokens))
    };
    vec![
        row.label.clone(),
        row.requests.to_string(),
        row.failures.to_string(),
        third,
        format_ms(row.ttft_p50_ms),
        format_tps(row.tps_avg),
        row.retries.to_string(),
        format_cost(row.cost_usd_ticks, row.cost_partial),
    ]
}

fn request_cells(row: &RequestRow) -> Vec<String> {
    vec![
        clock(row.ts_ms),
        row.model.clone(),
        row.session_label.clone(),
        format_tokens(row.input_tokens),
        format_tokens(row.output_tokens),
        format_ms(row.ttft_ms),
        format_tps(row.tps),
        if row.ok {
            "ok".to_string()
        } else {
            "fail".to_string()
        },
    ]
}

fn join_cells(cells: &[String]) -> String {
    let mut out = String::new();
    for (i, cell) in cells.iter().enumerate() {
        let width = if i == 0 { 18 } else { 8 };
        if i > 0 {
            out.push(' ');
        }
        out.push_str(&clip(cell, width));
        let pad = width.saturating_sub(cell.width().min(width));
        for _ in 0..pad {
            out.push(' ');
        }
    }
    out
}

fn sort_groups(rows: &mut [GroupRow], col: usize, desc: bool) {
    rows.sort_by(|a, b| {
        let order = match col {
            0 => a.label.cmp(&b.label),
            2 => a.failures.cmp(&b.failures),
            3 => a.input_tokens.cmp(&b.input_tokens),
            4 => a.ttft_p50_ms.unwrap_or(0).cmp(&b.ttft_p50_ms.unwrap_or(0)),
            5 => cmp_f64(a.tps_avg, b.tps_avg),
            6 => a.retries.cmp(&b.retries),
            7 => a
                .cost_usd_ticks
                .unwrap_or(0)
                .cmp(&b.cost_usd_ticks.unwrap_or(0)),
            _ => a.requests.cmp(&b.requests),
        };
        if desc { order.reverse() } else { order }
    });
}

fn sort_requests(rows: &mut [RequestRow], col: usize, desc: bool) {
    rows.sort_by(|a, b| {
        let order = match col {
            1 => a.model.cmp(&b.model),
            2 => a.session_label.cmp(&b.session_label),
            3 => a.input_tokens.cmp(&b.input_tokens),
            4 => a.output_tokens.cmp(&b.output_tokens),
            5 => a.ttft_ms.unwrap_or(0).cmp(&b.ttft_ms.unwrap_or(0)),
            6 => cmp_f64(a.tps, b.tps),
            7 => (a.ok as u8).cmp(&(b.ok as u8)),
            _ => a.ts_ms.cmp(&b.ts_ms),
        };
        if desc { order.reverse() } else { order }
    });
}

fn filtered_groups<'a>(rows: &'a [GroupRow], filter: &str) -> Vec<&'a GroupRow> {
    let needle = filter.trim().to_lowercase();
    rows.iter()
        .filter(|row| {
            needle.is_empty()
                || row.label.to_lowercase().contains(&needle)
                || row.key.to_lowercase().contains(&needle)
        })
        .collect()
}

fn filtered_requests<'a>(rows: &'a [RequestRow], filter: &str) -> Vec<&'a RequestRow> {
    let needle = filter.trim().to_lowercase();
    rows.iter()
        .filter(|row| {
            needle.is_empty()
                || row.model.to_lowercase().contains(&needle)
                || row.session_label.to_lowercase().contains(&needle)
                || row.session_id.to_lowercase().contains(&needle)
                || row
                    .error_kind
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains(&needle)
        })
        .collect()
}

fn fit_scroll(state: &mut StatsModalState, len: usize, visible: usize) {
    if len == 0 {
        state.cursor = 0;
        state.scroll = 0;
        return;
    }
    if state.cursor >= len {
        state.cursor = len - 1;
    }
    let window = visible.max(1);
    if state.cursor < state.scroll {
        state.scroll = state.cursor;
    } else if state.cursor >= state.scroll + window {
        state.scroll = state.cursor + 1 - window;
    }
}

fn footer() -> Vec<Shortcut<'static>> {
    vec![Shortcut {
        label: "otmsre tabs  1234 range  hd grain  f filter  space pause",
        clickable: false,
        id: 0,
    }]
}

fn range_label(range: TimeRange) -> &'static str {
    match range {
        TimeRange::All => "all",
        TimeRange::Today => "today",
        TimeRange::Days7 => "7d",
        TimeRange::Days30 => "30d",
    }
}

fn token_mix(input: u64, cache: u64, output: u64) -> String {
    let total = input.saturating_add(output).max(1);
    let width = 24usize;
    let cache_w = ((cache as f64 / total as f64) * width as f64).round() as usize;
    let out_w = ((output as f64 / total as f64) * width as f64).round() as usize;
    let cache_w = cache_w.min(width);
    let out_w = out_w.min(width.saturating_sub(cache_w));
    let in_w = width.saturating_sub(cache_w).saturating_sub(out_w);
    format!(
        "in {}  cache {}  out {}   {}{}{}",
        format_tokens(input),
        format_tokens(cache),
        format_tokens(output),
        "█".repeat(in_w),
        "▓".repeat(cache_w),
        "░".repeat(out_w)
    )
}

fn spark_line(
    buf: &mut Buffer,
    area: Rect,
    y: u16,
    label: &str,
    data: &[u64],
    color: ratatui::style::Color,
) -> u16 {
    if y >= area.y + area.height {
        return y;
    }
    put(buf, area.x, y, label, 6, Style::default().fg(color));
    let spark_area = Rect {
        x: area.x.saturating_add(6),
        y,
        width: area.width.saturating_sub(6),
        height: 1,
    };
    if spark_area.width > 0 && !data.is_empty() {
        Sparkline::default()
            .data(data)
            .style(Style::default().fg(color))
            .render(spark_area, buf);
    }
    y.saturating_add(1)
}

fn blocks(value: f64, max: f64, width: usize) -> String {
    if width == 0 || max <= 0.0 {
        return String::new();
    }
    let filled = ((value.max(0.0) / max) * width as f64).round() as usize;
    let filled = filled.min(width);
    let mut out = String::new();
    for i in 0..width {
        out.push(if i < filled { '█' } else { '·' });
    }
    out
}

fn ttft_style(avg_ms: Option<f64>, theme: &Theme) -> Style {
    let ms = avg_ms.unwrap_or(0.0);
    let color = if ms <= 5_000.0 {
        theme.accent_success
    } else if ms <= 120_000.0 {
        theme.warning
    } else {
        theme.accent_error
    };
    Style::default().fg(color)
}

fn bucket_label(ts_ms: i64, daily: bool) -> String {
    let Some(dt) = chrono::DateTime::from_timestamp_millis(ts_ms) else {
        return "-".to_string();
    };
    let local = dt.with_timezone(&chrono::Local);
    if daily {
        local.format("%m-%d").to_string()
    } else {
        local.format("%d %H").to_string()
    }
}

fn clock(ts_ms: i64) -> String {
    let Some(dt) = chrono::DateTime::from_timestamp_millis(ts_ms) else {
        return "-".to_string();
    };
    dt.with_timezone(&chrono::Local)
        .format("%m-%d %H:%M")
        .to_string()
}

fn col_width(index: usize, count: usize, total: u16) -> u16 {
    if count == 0 {
        return 0;
    }
    let each = total / count as u16;
    if index + 1 == count {
        total.saturating_sub(each.saturating_mul(count.saturating_sub(1) as u16))
    } else {
        each
    }
}

fn clip(text: &str, width: usize) -> String {
    if text.width() <= width {
        return text.to_string();
    }
    let mut out = String::new();
    let mut used = 0;
    for ch in text.chars() {
        let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if used + w > width.saturating_sub(1) {
            break;
        }
        out.push(ch);
        used += w;
    }
    out.push('…');
    out
}

fn cmp_f64(a: Option<f64>, b: Option<f64>) -> std::cmp::Ordering {
    a.unwrap_or(0.0)
        .partial_cmp(&b.unwrap_or(0.0))
        .unwrap_or(std::cmp::Ordering::Equal)
}

fn put(buf: &mut Buffer, x: u16, y: u16, text: &str, width: u16, style: Style) {
    if width == 0 {
        return;
    }
    buf.set_line(
        x,
        y,
        &Line::from(Span::styled(clip(text, width as usize), style)),
        width,
    );
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|dur| i64::try_from(dur.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}
