//! Tab bar: single-line row of tab labels with active underline indicator.

use ratatui::{
    prelude::*,
    text::{Line, Span},
};

use super::theme;

pub const TAB_LABELS: &[&str] = &["Overview", "Sessions", "Analysis"];

/// Render the tab bar + separator line with an active-tab indicator.
/// Takes 2 rows: line 0 = tabs, line 1 = separator with underline accent.
pub fn render_tab_bar(frame: &mut Frame, area: Rect, active_index: usize) {
    let [tab_line, sep_line] =
        Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).areas(area);

    let mut spans: Vec<Span> = vec![Span::raw(" ")];
    let mut col: usize = 1;
    let mut active_start: usize = 0;
    let mut active_end: usize = 0;

    for (i, label) in TAB_LABELS.iter().enumerate() {
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

    frame.render_widget(Line::from(spans), tab_line);

    render_separator_with_accent(frame, sep_line, active_start, active_end);
}

/// Render a separator line that uses heavy chars under the active tab.
fn render_separator_with_accent(frame: &mut Frame, area: Rect, start: usize, end: usize) {
    let w = area.width as usize;
    let mut sep_spans: Vec<Span> = Vec::new();

    if start > 0 {
        let n = start.min(w);
        sep_spans.push(Span::styled(
            theme::SEP_LIGHT.repeat(n),
            theme::separator_style(),
        ));
    }
    if start < w {
        let accent_len = (end.min(w)).saturating_sub(start);
        sep_spans.push(Span::styled(
            theme::SEP_HEAVY.repeat(accent_len),
            theme::tab_active_style(),
        ));
    }
    if end < w {
        sep_spans.push(Span::styled(
            theme::SEP_LIGHT.repeat(w.saturating_sub(end)),
            theme::separator_style(),
        ));
    }

    frame.render_widget(Line::from(sep_spans), area);
}
