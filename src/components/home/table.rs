//! Session list table: cyberpunk style with zebra stripes, selection bar,
//! and expandable per-model rows.

use ratatui::{
    layout::Constraint,
    text::{Line, Span, Text},
    widgets::{Cell, Row, TableState},
};

use crate::collector::transcript::TranscriptData;
use crate::components::common::theme;
use crate::utils::format::{format_cost, format_last_active, format_tokens};

use super::view::ExpandState;

pub const SESSION_NAME_MAX_LEN: usize = 36;

pub const COLUMN_WIDTHS: [Constraint; 9] = [
    Constraint::Length(3), // selection bar + expand indicator
    Constraint::Length(7), // source
    Constraint::Min(20),   // session name
    Constraint::Min(14),   // models
    Constraint::Length(8), // input
    Constraint::Length(8), // output
    Constraint::Length(8), // total
    Constraint::Length(8), // last active
    Constraint::Length(7), // cost
];

pub fn table_header() -> Row<'static> {
    Row::new(vec![
        Cell::from(""),
        Cell::from(" Source"),
        Cell::from(" Session"),
        Cell::from(" Models"),
        Cell::from(" Input"),
        Cell::from(" Output"),
        Cell::from(" Total"),
        Cell::from(" Active"),
        Cell::from(" Cost"),
    ])
    .style(theme::header_style())
    .height(1)
    .bottom_margin(1)
}

pub fn table_row(
    index: usize,
    display_name: &str,
    data: &TranscriptData,
    expand_state: &ExpandState,
    is_selected: bool,
) -> Row<'static> {
    let total = data.input_tokens + data.output_tokens;
    let session_name = display_name.to_string();
    let model_count = data.models.len();
    let expanded = expand_state.is_expanded(index);
    let source_kind = data.source;

    let expand_icon = if model_count <= 1 {
        "  "
    } else if expanded {
        "▾ "
    } else {
        "▸ "
    };

    let bar = if is_selected { "│" } else { " " };
    let indicator = format!("{}{}", bar, expand_icon);

    let models_str = if model_count == 1 {
        data.models.iter().next().cloned().unwrap_or_default()
    } else {
        format!("{} models", model_count)
    };

    let active_str = format_last_active(data.end_time.as_deref().or(data.start_time.as_deref()));
    let cost_str = format_cost(data.estimated_cost_usd);

    // Build multi-line cells if expanded
    if expanded && model_count > 1 {
        let mut name_lines = vec![Line::from(session_name)];
        let mut model_lines = vec![Line::from(models_str)];
        let mut input_lines = vec![Line::from(format_tokens(data.input_tokens))];
        let mut output_lines = vec![Line::from(format_tokens(data.output_tokens))];
        let mut total_lines = vec![Line::from(format_tokens(total))];
        let mut indicator_lines = vec![Line::from(Span::styled(
            indicator,
            if is_selected {
                theme::selection_bar_style()
            } else {
                theme::fold_style()
            },
        ))];
        let mut source_lines = vec![Line::from(Span::styled(
            format!("{}", source_kind),
            theme::value_style(),
        ))];
        let mut active_lines = vec![Line::from(active_str)];
        let cost_sty = theme::cost_style(data.estimated_cost_usd);
        let mut cost_lines = vec![Line::from(Span::styled(cost_str, cost_sty))];

        let models_sorted: Vec<_> = {
            let mut v: Vec<_> = data.per_model.iter().collect();
            v.sort_by(|a, b| {
                (b.1.input_tokens + b.1.output_tokens).cmp(&(a.1.input_tokens + a.1.output_tokens))
            });
            v
        };

        for (i, (model, stats)) in models_sorted.iter().enumerate() {
            let connector = if i == models_sorted.len() - 1 {
                "  └ "
            } else {
                "  ├ "
            };
            let sub_bar = if is_selected { "│" } else { " " };
            indicator_lines.push(Line::from(Span::styled(
                format!("{} ", sub_bar),
                if is_selected {
                    theme::selection_bar_style()
                } else {
                    theme::tree_style()
                },
            )));
            name_lines.push(Line::from(Span::styled(connector, theme::tree_style())));
            model_lines.push(Line::from(Span::styled(
                model.to_string(),
                theme::model_style(),
            )));
            input_lines.push(Line::from(format_tokens(stats.input_tokens)));
            output_lines.push(Line::from(format_tokens(stats.output_tokens)));
            let sub_total = stats.input_tokens + stats.output_tokens;
            total_lines.push(Line::from(format_tokens(sub_total)));
            source_lines.push(Line::from(""));
            active_lines.push(Line::from(""));
            cost_lines.push(Line::from(""));
        }

        let row_height = (1 + models_sorted.len()) as u16;

        let base_style = if index % 2 == 1 {
            theme::zebra_style()
        } else {
            theme::body_style()
        };

        Row::new(vec![
            Cell::from(Text::from(indicator_lines)),
            Cell::from(Text::from(source_lines)),
            Cell::from(Text::from(name_lines)),
            Cell::from(Text::from(model_lines)),
            Cell::from(Text::from(input_lines)),
            Cell::from(Text::from(output_lines)),
            Cell::from(Text::from(total_lines)),
            Cell::from(Text::from(active_lines)),
            Cell::from(Text::from(cost_lines)),
        ])
        .style(base_style)
        .height(row_height)
        .bottom_margin(1)
    } else {
        let base_style = if index % 2 == 1 {
            theme::zebra_style()
        } else {
            theme::body_style()
        };

        Row::new(vec![
            Cell::from(Span::styled(
                indicator,
                if is_selected {
                    theme::selection_bar_style()
                } else {
                    theme::fold_style()
                },
            )),
            Cell::from(Span::styled(
                format!("{}", source_kind),
                theme::value_style(),
            )),
            Cell::from(session_name),
            Cell::from(models_str),
            Cell::from(format_tokens(data.input_tokens)),
            Cell::from(format_tokens(data.output_tokens)),
            Cell::from(format_tokens(total)),
            Cell::from(active_str),
            Cell::from(Span::styled(
                cost_str,
                theme::cost_style(data.estimated_cost_usd),
            )),
        ])
        .style(base_style)
        .height(2)
        .bottom_margin(0)
    }
}

pub fn selection_next(selected: Option<usize>, len: usize) -> Option<usize> {
    if len == 0 {
        return None;
    }
    let i = selected.unwrap_or(0);
    Some((i + 1).min(len.saturating_sub(1)))
}

pub fn selection_previous(selected: Option<usize>, _len: usize) -> Option<usize> {
    match selected {
        Some(0) | None => Some(0),
        Some(i) => Some(i - 1),
    }
}

pub fn set_selection(state: &mut TableState, index: Option<usize>) {
    state.select(index);
}
