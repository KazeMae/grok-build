//! Top bar component: renders cwd and git info.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Widget};

use std::path::{Path, PathBuf};

use crate::git_info;
use crate::render::line_utils::truncate_line;
use crate::theme::Theme;
use crate::views::location::{location_parts, worktree_badge};

pub fn render_top_bar(
    area: Rect,
    buf: &mut Buffer,
    theme: &Theme,
    announcement: Option<&xai_grok_announcements::RemoteAnnouncement>,
    locale: Option<&crate::locale::LocaleContext>,
) {
    let line = truncate_line(
        location_line_with_locale(theme, locale),
        area.width as usize,
    );
    let line_width = line.width() as u16;
    buf.set_line(area.x, area.y, &line, line_width.min(area.width));

    if let Some(a) = announcement
        && let Some(text) = a.message.as_deref()
        && area.height > 1
    {
        let text_style = Style::default().fg(theme.text_primary);
        let line = Line::from(Span::styled(text, text_style));
        Paragraph::new(line).render(
            Rect {
                y: area.y + 1,
                height: area.height.saturating_sub(1),
                ..area
            },
            buf,
        );
    }
}

/// Build the `{git branch} {worktree} {cwd}` line for the welcome top bar, reading the live process cwd.
/// The caller width-truncates the returned line.
pub(crate) fn location_line(theme: &Theme) -> Line<'static> {
    location_line_with_locale(theme, None)
}

pub(crate) fn location_line_with_locale(
    theme: &Theme,
    locale: Option<&crate::locale::LocaleContext>,
) -> Line<'static> {
    location_line_at_with_locale(theme, &process_cwd(), locale)
}

/// As [`location_line`], but for an explicit `cwd`.
/// The dashboard header passes its staged `app.cwd` so the line tracks a `/cd` immediately.
/// That holds before (or even if) `Effect::SetWorkingDir` moves the process cwd.
/// Safe to call during render: it reads the per-cwd git cache and never blocks or spawns `git`.
/// The caller width-truncates the returned line.
pub(crate) fn location_line_at(theme: &Theme, cwd: &Path) -> Line<'static> {
    location_line_at_with_locale(theme, cwd, None)
}

pub(crate) fn location_line_at_with_locale(
    theme: &Theme,
    cwd: &Path,
    locale: Option<&crate::locale::LocaleContext>,
) -> Line<'static> {
    let info_style = Style::default().fg(theme.gray);
    let parts = location_parts(cwd);

    let mut spans: Vec<Span> = Vec::new();
    if let Some(branch) = parts.branch.as_deref() {
        let icon = git_info::branch_icon();
        let branch_label = if branch == "detached" {
            locale.map_or("detached", |locale| {
                locale.named_static_text("welcome.location.detached", "detached")
            })
        } else {
            branch
        };
        let git_style = Style::default()
            .fg(theme.text_primary)
            .add_modifier(Modifier::DIM);
        spans.push(Span::styled(format!("{icon} {branch_label}"), git_style));
        spans.push(Span::styled(" ", info_style));
    }
    if parts.is_worktree {
        if let Some(locale) = locale {
            let badge = locale.named_static_text("welcome.location.worktree_badge", "worktree ");
            spans.push(Span::styled(badge, Style::default().fg(theme.accent_user)));
        } else {
            spans.push(worktree_badge(theme));
        }
    }
    let cwd_style = Style::default().fg(theme.gray_dim);
    let cwd_display = localize_cwd_display(&parts.cwd_display, locale);
    spans.push(Span::styled(cwd_display, cwd_style));
    Line::from(spans)
}

fn process_cwd() -> PathBuf {
    std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
}

/// Localize the `(worktree of …)` suffix already formatted by [`location_parts`].
fn localize_cwd_display(
    cwd_display: &str,
    locale: Option<&crate::locale::LocaleContext>,
) -> String {
    const SUFFIX: &str = " (worktree of ";
    let Some(locale) = locale else {
        return cwd_display.to_string();
    };
    let Some(idx) = cwd_display.rfind(SUFFIX) else {
        return cwd_display.to_string();
    };
    if !cwd_display.ends_with(')') {
        return cwd_display.to_string();
    }
    let display = &cwd_display[..idx];
    let main_repo = &cwd_display[idx + SUFFIX.len()..cwd_display.len() - 1];
    locale
        .named_text(
            "welcome.location.worktree_of",
            "{display} (worktree of {main_repo})",
        )
        .replace("{display}", display)
        .replace("{main_repo}", main_repo)
}
