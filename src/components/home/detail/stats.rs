//! Stats tab: session info, token breakdown, model usage, and tool call distribution.

use ratatui::prelude::*;
use ratatui::text::Line;

use crate::collector::transcript::TranscriptData;
use crate::components::common::theme;
use crate::utils::format::{format_cost, format_duration, format_tokens};

fn format_timestamp(ts: &str) -> String {
    if ts.len() >= 16 {
        let date_part = &ts[..10];
        let time_part = &ts[11..16];
        format!("{} {}", date_part, time_part)
    } else {
        ts.to_string()
    }
}

/// Build stats content as lines for scrollable rendering.
pub fn detail_stats_content(data: &TranscriptData, width: u16) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let w = width as usize;

    // ── Session + Tokens (side by side) ──
    let col_w = w / 2;
    let label_w: usize = 14;
    let value_w = col_w.saturating_sub(label_w + 2);

    let dur = format_duration(data.duration_ms);
    let cost = format_cost(data.estimated_cost_usd);
    let start_display = data
        .start_time
        .as_ref()
        .map(|s| format_timestamp(s))
        .unwrap_or_else(|| "—".to_string());
    let branch = data.git_branch.as_deref().unwrap_or("—").to_string();
    let version = data.agent_version.as_deref().unwrap_or("—").to_string();
    let msgs = format!(
        "{}u / {}a",
        data.user_message_count, data.assistant_message_count
    );

    let total_cache = data.cache_creation_tokens + data.cache_read_tokens;
    let hit_rate = if total_cache > 0 {
        format!(
            "{:.0}%",
            data.cache_read_tokens as f64 / total_cache as f64 * 100.0
        )
    } else {
        "—".to_string()
    };

    let session_rows: Vec<(&str, String, Style)> = vec![
        ("Duration", dur, theme::stat_number_style()),
        ("Started", start_display, theme::stat_number_style()),
        (
            "Turns",
            format!("{}", data.turn_count),
            theme::stat_number_style(),
        ),
        ("Cost", cost, theme::cost_style(data.estimated_cost_usd)),
        ("Branch", branch, theme::stat_number_style()),
        ("Version", version, theme::stat_number_style()),
        ("Messages", msgs, theme::stat_secondary_style()),
    ];

    let token_rows: Vec<(&str, String, Style)> = vec![
        (
            "Input",
            format_tokens(data.input_tokens),
            theme::stat_number_style(),
        ),
        (
            "Output",
            format_tokens(data.output_tokens),
            theme::stat_number_style(),
        ),
        (
            "Cache Write",
            format_tokens(data.cache_creation_tokens),
            theme::stat_number_style(),
        ),
        (
            "Cache Read",
            format_tokens(data.cache_read_tokens),
            theme::stat_number_style(),
        ),
        ("Hit Rate", hit_rate, theme::stat_number_style()),
    ];

    // Title row
    lines.push(Line::from(vec![
        Span::styled(
            format!(" {:<width$}", "Session", width = col_w - 1),
            theme::section_title_style(),
        ),
        Span::styled(" Tokens", theme::section_title_style()),
    ]));

    // Separator row
    let sep_w = col_w.saturating_sub(4);
    lines.push(Line::from(vec![
        Span::styled(
            format!(
                "  {:<width$}",
                theme::SEP_DASH.repeat(sep_w),
                width = col_w - 2
            ),
            theme::separator_style(),
        ),
        Span::styled(
            format!("  {}", theme::SEP_DASH.repeat(sep_w)),
            theme::separator_style(),
        ),
    ]));

    // Content rows side by side
    let max_rows = session_rows.len().max(token_rows.len());
    for i in 0..max_rows {
        let mut spans = Vec::new();
        if i < session_rows.len() {
            let (lbl, val, sty) = &session_rows[i];
            spans.push(Span::styled(
                format!("  {:<label_w$}", lbl),
                theme::label_style(),
            ));
            spans.push(Span::styled(format!("{:<value_w$}", val), *sty));
        } else {
            spans.push(Span::raw(" ".repeat(col_w)));
        }
        if i < token_rows.len() {
            let (lbl, val, sty) = &token_rows[i];
            spans.push(Span::styled(
                format!("  {:<label_w$}", lbl),
                theme::label_style(),
            ));
            spans.push(Span::styled(val.clone(), *sty));
        }
        lines.push(Line::from(spans));
    }

    lines.push(Line::from(""));

    // ── Models ──
    render_models_section(&mut lines, data, w);

    lines.push(Line::from(""));

    // ── Tool Calls ──
    render_tools_section(&mut lines, data, w);

    lines
}

fn render_models_section(lines: &mut Vec<Line<'static>>, data: &TranscriptData, w: usize) {
    let mut models_sorted: Vec<_> = data.per_model.iter().collect();
    models_sorted.sort_by(|a, b| {
        (b.1.input_tokens + b.1.output_tokens).cmp(&(a.1.input_tokens + a.1.output_tokens))
    });

    let max_total = models_sorted
        .first()
        .map(|(_, s)| s.input_tokens + s.output_tokens)
        .unwrap_or(1);
    let model_name_w = models_sorted
        .iter()
        .map(|(m, _)| m.len())
        .max()
        .unwrap_or(12)
        .max(12)
        .min(w.saturating_sub(34));
    let bar_w = w.saturating_sub(model_name_w + 34).clamp(4, 20);

    lines.push(Line::from(Span::styled(
        " Models ",
        theme::section_title_style(),
    )));
    lines.push(Line::from(Span::styled(
        format!("  {}", theme::SEP_DASH.repeat(w.saturating_sub(4))),
        theme::separator_style(),
    )));

    if models_sorted.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No model data",
            theme::fold_style(),
        )));
    } else {
        for (model, stats) in &models_sorted {
            let total = stats.input_tokens + stats.output_tokens;
            let ratio = (total as f64 / max_total as f64).min(1.0);
            let filled = (ratio * bar_w as f64).round() as usize;
            let empty = bar_w.saturating_sub(filled);

            let display_model: &str = if model.len() > model_name_w {
                &model[..model_name_w]
            } else {
                model
            };

            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {:<width$}  ", display_model, width = model_name_w),
                    theme::model_style(),
                ),
                Span::styled("█".repeat(filled), theme::bar_filled_style()),
                Span::styled("░".repeat(empty), theme::bar_empty_style()),
                Span::styled(
                    format!(
                        "  In {:>6}  Out {:>6}",
                        format_tokens(stats.input_tokens),
                        format_tokens(stats.output_tokens)
                    ),
                    theme::stat_number_style(),
                ),
            ]));
        }
    }
}

fn render_tools_section(lines: &mut Vec<Line<'static>>, data: &TranscriptData, w: usize) {
    let tool_total = data.tool_call_total;
    let mut tools_sorted: Vec<_> = data.tool_call_by_type.iter().collect();
    tools_sorted.sort_by(|a, b| b.1.cmp(a.1));

    let max_count = tools_sorted.first().map(|(_, v)| **v).unwrap_or(1);
    let tool_name_w = tools_sorted
        .iter()
        .map(|(n, _)| n.len())
        .max()
        .unwrap_or(10)
        .max(10)
        .min(w.saturating_sub(24));
    let tool_bar_w = w.saturating_sub(tool_name_w + 24).clamp(4, 24);

    lines.push(Line::from(vec![
        Span::styled(" Tool Calls ", theme::section_title_style()),
        Span::styled(
            format!("{}", tools_sorted.len()),
            theme::stat_number_style(),
        ),
        Span::styled(" types  ", theme::label_style()),
        Span::styled(format!("{}", tool_total), theme::stat_number_style()),
        Span::styled(" calls", theme::label_style()),
    ]));
    lines.push(Line::from(Span::styled(
        format!("  {}", theme::SEP_DASH.repeat(w.saturating_sub(4))),
        theme::separator_style(),
    )));

    if tools_sorted.is_empty() {
        lines.push(Line::from(Span::styled(
            "  No tool calls",
            theme::fold_style(),
        )));
    } else {
        for (name, count) in &tools_sorted {
            let ratio = (**count as f64 / max_count as f64).min(1.0);
            let pct = if tool_total > 0 {
                **count as f64 / tool_total as f64 * 100.0
            } else {
                0.0
            };
            let filled = (ratio * tool_bar_w as f64).round() as usize;
            let empty = tool_bar_w.saturating_sub(filled);

            let display_name: &str = if name.len() > tool_name_w {
                &name[..tool_name_w]
            } else {
                name
            };

            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {:<width$}  ", display_name, width = tool_name_w),
                    theme::label_style(),
                ),
                Span::styled("█".repeat(filled), theme::bar_filled_style()),
                Span::styled("░".repeat(empty), theme::bar_empty_style()),
                Span::styled(format!(" {:>5} ", count), theme::value_style()),
                Span::styled(format!("{:>3.0}%", pct), theme::stat_secondary_style()),
            ]));
        }
    }
}
