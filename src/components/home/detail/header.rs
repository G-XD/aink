//! Detail view header: project name + sub-tab bar with active underline indicator.

use std::path::Path;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::prelude::*;
use ratatui::text::Line;

use crate::collector::transcript::TranscriptData;
use crate::components::common::theme;
use crate::components::home::view::{DETAIL_TAB_LABELS, DetailTab};
use crate::utils::project_name;

const SESSION_NAME_MAX_LEN: usize = 36;

/// Render combined header: project name (left) + sub-tab labels (right),
/// with separator below that highlights the active tab region.
pub fn render_detail_header(
    frame: &mut Frame,
    area: Rect,
    path: &Path,
    data: &TranscriptData,
    active_tab: DetailTab,
) {
    let [header_line, sep_line] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);

    let name = project_name::session_display_name(path, data, SESSION_NAME_MAX_LEN);
    let active_index = active_tab.index();

    let source_badge = format!("[{}]", data.source);
    let breadcrumb = "Sessions \u{203a} ";
    let prefix_len =
        breadcrumb.chars().count() + source_badge.chars().count() + 1 + name.chars().count() + 4;

    let mut spans: Vec<Span> = vec![
        Span::styled(breadcrumb, theme::tab_inactive_style()),
        Span::styled(source_badge, theme::source_style(data.source)),
        Span::raw(" "),
        Span::styled(name, theme::section_title_style()),
        Span::raw("    "),
    ];

    let mut col = prefix_len;
    let mut active_start: usize = 0;
    let mut active_end: usize = 0;

    for (i, label) in DETAIL_TAB_LABELS.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("   ", theme::tab_key_style()));
            col += 3;
        }

        let tab_start = col;
        let first = &label[..1];
        let rest = &label[1..];
        let tab_width = 1 + label.len() + 1;

        if i == active_index {
            active_start = tab_start;
            active_end = tab_start + tab_width;
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                first,
                theme::tab_active_style().add_modifier(Modifier::UNDERLINED),
            ));
            spans.push(Span::styled(
                format!("{} ", rest),
                theme::tab_active_style(),
            ));
        } else {
            spans.push(Span::raw(" "));
            spans.push(Span::styled(
                first,
                theme::tab_inactive_style().add_modifier(Modifier::UNDERLINED),
            ));
            spans.push(Span::styled(
                format!("{} ", rest),
                theme::tab_inactive_style(),
            ));
        }
        col += tab_width;
    }

    frame.render_widget(Line::from(spans), header_line);

    // Separator with heavy accent under the active tab
    let w = area.width as usize;
    let mut sep_spans: Vec<Span> = Vec::new();

    if active_start > 0 {
        let n = active_start.min(w);
        sep_spans.push(Span::styled(
            theme::SEP_LIGHT.repeat(n),
            theme::separator_style(),
        ));
    }
    if active_start < w {
        let accent_len = (active_end.min(w)).saturating_sub(active_start);
        sep_spans.push(Span::styled(
            theme::SEP_HEAVY.repeat(accent_len),
            theme::tab_active_style(),
        ));
    }
    if active_end < w {
        sep_spans.push(Span::styled(
            theme::SEP_LIGHT.repeat(w.saturating_sub(active_end)),
            theme::separator_style(),
        ));
    }

    frame.render_widget(Line::from(sep_spans), sep_line);
}
