//! Overview tab: aggregate stats across all sessions.
//! Block layout: stat cards, models/projects columns, tool distribution.

use std::collections::HashMap;
use std::sync::Arc;

use ratatui::{
    layout::{Constraint, Layout, Rect},
    prelude::*,
    text::{Line, Span},
    widgets::Paragraph,
};

use crate::collector::transcript::SessionList;
use crate::components::common::theme;
use crate::utils::format::{format_cost, format_duration, format_tokens};
use crate::utils::project_name;

use super::Component;

fn render_separator_h(frame: &mut Frame, area: Rect) {
    let sep = theme::SEP_DASH.repeat(area.width as usize);
    let line = Paragraph::new(Line::from(Span::styled(sep, theme::separator_style())));
    frame.render_widget(line, area);
}

/// Per-source aggregate statistics.
struct SourceStat {
    name: String,
    session_count: usize,
    total_tokens: u64,
    estimated_cost: f64,
}

/// Pre-computed aggregate data for rendering, updated only when transcripts change.
struct OverviewCache {
    session_count: usize,
    source_stats: Vec<SourceStat>,
    total_input: u64,
    total_output: u64,
    total_all: u64,
    total_tools: u64,
    total_duration_ms: u64,
    total_cost: f64,
    models_sorted: Vec<(String, u64)>,
    project_tokens: Vec<(String, u64)>,
    tools_sorted: Vec<(String, u64)>,
}

#[derive(Default)]
pub struct Overview {
    cache: Option<OverviewCache>,
}

impl Overview {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_transcripts(&mut self, data: Arc<SessionList>) {
        if data.is_empty() {
            self.cache = None;
            return;
        }

        let mut total_input: u64 = 0;
        let mut total_output: u64 = 0;
        let mut total_tools: u64 = 0;
        let mut total_duration_ms: u64 = 0;
        let mut total_cost: f64 = 0.0;
        let mut model_tokens: HashMap<String, u64> = HashMap::new();
        let mut tool_counts: HashMap<String, u64> = HashMap::new();
        let mut project_tokens_map: HashMap<String, u64> = HashMap::new();
        let mut source_accum: HashMap<String, (usize, u64, f64)> = HashMap::new();

        for (path, data) in data.iter() {
            let src_key = format!("{}", data.source);
            let src = source_accum.entry(src_key).or_insert((0, 0, 0.0));
            src.0 += 1;
            src.1 += data.input_tokens + data.output_tokens;
            src.2 += data.estimated_cost_usd;
            total_input += data.input_tokens;
            total_output += data.output_tokens;
            total_tools += data.tool_call_total;
            total_duration_ms += data.duration_ms;
            total_cost += data.estimated_cost_usd;

            for model in &data.models {
                *model_tokens.entry(model.clone()).or_insert(0) +=
                    data.input_tokens + data.output_tokens;
            }

            for (tool, count) in &data.tool_call_by_type {
                *tool_counts.entry(tool.clone()).or_insert(0) += count;
            }

            let name = project_name::session_display_name_with_slug(path, data.slug.as_deref(), 24);
            *project_tokens_map.entry(name).or_insert(0) += data.input_tokens + data.output_tokens;
        }

        let total_all = total_input + total_output;

        let mut models_sorted: Vec<_> = model_tokens.into_iter().collect();
        models_sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        let mut project_tokens: Vec<_> = project_tokens_map.into_iter().collect();
        project_tokens.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        let mut tools_sorted: Vec<_> = tool_counts.into_iter().collect();
        tools_sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        let mut source_stats: Vec<SourceStat> = source_accum
            .into_iter()
            .map(|(name, (count, tokens, cost))| SourceStat {
                name,
                session_count: count,
                total_tokens: tokens,
                estimated_cost: cost,
            })
            .collect();
        source_stats.sort_by(|a, b| b.session_count.cmp(&a.session_count));

        self.cache = Some(OverviewCache {
            session_count: data.len(),
            source_stats,
            total_input,
            total_output,
            total_all,
            total_tools,
            total_duration_ms,
            total_cost,
            models_sorted,
            project_tokens,
            tools_sorted,
        });
    }
}

impl Component for Overview {
    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        let Some(c) = &self.cache else {
            let msg = Paragraph::new("  No data yet. Switch to Sessions tab and load transcripts.")
                .style(theme::empty_msg_style());
            frame.render_widget(msg, area);
            return Ok(());
        };

        // ── Main vertical layout ──
        let source_row_count = c.source_stats.len().min(6);
        let rows = Layout::vertical([
            Constraint::Length(5),
            Constraint::Length(1),
            Constraint::Length((source_row_count + 2) as u16),
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
            Constraint::Fill(1),
        ])
        .split(area);

        // ── Row 0: Stat cards (5 equal columns) ──
        let stat_cols = Layout::horizontal([
            Constraint::Ratio(1, 5),
            Constraint::Ratio(1, 5),
            Constraint::Ratio(1, 5),
            Constraint::Ratio(1, 5),
            Constraint::Ratio(1, 5),
        ])
        .split(rows[0]);

        let cards: Vec<(&str, String, Style, Option<Line<'static>>)> = vec![
            (
                "Sessions",
                format!("{}", c.session_count),
                theme::stat_number_style(),
                None,
            ),
            (
                "Tokens",
                format_tokens(c.total_all),
                theme::stat_number_style(),
                Some(
                    Line::from(vec![
                        Span::styled("In ", theme::label_style()),
                        Span::styled(format_tokens(c.total_input), theme::stat_secondary_style()),
                        Span::styled("  Out ", theme::label_style()),
                        Span::styled(format_tokens(c.total_output), theme::stat_secondary_style()),
                    ])
                    .alignment(Alignment::Center),
                ),
            ),
            (
                "Tools",
                format!("{}", c.total_tools),
                theme::stat_number_style(),
                None,
            ),
            (
                "Duration",
                format_duration(c.total_duration_ms),
                theme::stat_number_style(),
                None,
            ),
            (
                "Cost",
                format_cost(c.total_cost),
                theme::cost_style(c.total_cost),
                None,
            ),
        ];

        for (i, (label, value, style, subtitle)) in cards.iter().enumerate() {
            let mut lines = vec![
                Line::from(""),
                Line::from(Span::styled(*label, theme::section_title_style()))
                    .alignment(Alignment::Center),
                Line::from(Span::styled(value.clone(), *style)).alignment(Alignment::Center),
            ];
            if let Some(sub) = subtitle {
                lines.push(sub.clone());
            } else {
                lines.push(Line::from(""));
            }
            frame.render_widget(Paragraph::new(lines), stat_cols[i]);
        }

        render_separator_h(frame, rows[1]);

        // ── Row 2: Sources ──
        {
            let col_w = rows[2].width as usize;
            let bar_width = 20usize.min(col_w.saturating_sub(50));
            let max_sessions = c.source_stats.first().map(|s| s.session_count).unwrap_or(1);

            let name_width = c
                .source_stats
                .iter()
                .take(source_row_count)
                .map(|s| s.name.len())
                .max()
                .unwrap_or(8)
                .max(8)
                .min(col_w.saturating_sub(50));

            let mut lines: Vec<Line<'static>> = Vec::new();
            lines.push(Line::from(Span::styled(
                "  Sources",
                theme::section_title_style(),
            )));
            lines.push(Line::from(""));

            for stat in c.source_stats.iter().take(source_row_count) {
                let ratio = (stat.session_count as f64 / max_sessions as f64).min(1.0);
                let filled = (ratio * bar_width as f64).round() as usize;
                let empty = bar_width.saturating_sub(filled);
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        format!("{:<width$}  ", stat.name, width = name_width),
                        theme::value_style(),
                    ),
                    Span::styled("\u{2588}".repeat(filled), theme::bar_filled_style()),
                    Span::styled("\u{2591}".repeat(empty), theme::bar_empty_style()),
                    Span::styled(
                        format!(" {:>4} sessions", stat.session_count),
                        theme::stat_number_style(),
                    ),
                    Span::styled(
                        format!("  {:>8}", format_tokens(stat.total_tokens)),
                        theme::stat_secondary_style(),
                    ),
                    Span::styled(
                        format!("  {}", format_cost(stat.estimated_cost)),
                        theme::cost_style(stat.estimated_cost),
                    ),
                ]));
            }

            frame.render_widget(Paragraph::new(lines), rows[2]);
        }

        render_separator_h(frame, rows[3]);

        // ── Row 4: Models (left) / Top Projects (right) ──
        let mid_cols = Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
            .split(rows[4]);

        {
            let max_rows = mid_cols[0].height.saturating_sub(2) as usize;
            let col_w = mid_cols[0].width as usize;

            let model_name_width = c
                .models_sorted
                .iter()
                .take(max_rows)
                .map(|(m, _)| m.len())
                .max()
                .unwrap_or(12)
                .max(12)
                .min(col_w.saturating_sub(20));

            let mut lines: Vec<Line<'static>> = Vec::new();
            lines.push(Line::from(Span::styled(
                "  Models",
                theme::section_title_style(),
            )));
            lines.push(Line::from(""));

            for (model, tokens) in c.models_sorted.iter().take(max_rows) {
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        format!("{:<width$}  ", model, width = model_name_width),
                        theme::model_style(),
                    ),
                    Span::styled(
                        format!("{:>10}", format_tokens(*tokens)),
                        theme::stat_number_style(),
                    ),
                ]));
            }

            frame.render_widget(Paragraph::new(lines), mid_cols[0]);
        }

        {
            let max_rows = mid_cols[1].height.saturating_sub(2) as usize;
            let col_w = mid_cols[1].width as usize;
            let bar_width = 16usize.min(col_w.saturating_sub(30));
            let max_project = c.project_tokens.first().map(|(_, t)| *t).unwrap_or(1);

            let project_name_width = c
                .project_tokens
                .iter()
                .take(max_rows)
                .map(|(n, _)| n.len())
                .max()
                .unwrap_or(12)
                .max(12)
                .min(col_w.saturating_sub(30));

            let mut lines: Vec<Line<'static>> = Vec::new();
            lines.push(Line::from(Span::styled(
                "  Top Projects",
                theme::section_title_style(),
            )));
            lines.push(Line::from(""));

            for (name, tokens) in c.project_tokens.iter().take(max_rows) {
                let ratio = (*tokens as f64 / max_project as f64).min(1.0);
                let filled = (ratio * bar_width as f64).round() as usize;
                let empty = bar_width.saturating_sub(filled);
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        format!("{:<width$}  ", name, width = project_name_width),
                        theme::value_style(),
                    ),
                    Span::styled("\u{2588}".repeat(filled), theme::bar_filled_style()),
                    Span::styled("\u{2591}".repeat(empty), theme::bar_empty_style()),
                    Span::styled(
                        format!(" {:>8}", format_tokens(*tokens)),
                        theme::stat_number_style(),
                    ),
                ]));
            }

            frame.render_widget(Paragraph::new(lines), mid_cols[1]);
        }

        render_separator_h(frame, rows[5]);

        // ── Row 6: Tool Distribution (full width) ──
        {
            let max_rows = rows[6].height.saturating_sub(2) as usize;
            let col_w = rows[6].width as usize;
            let bar_width = 24usize.min(col_w.saturating_sub(30));
            let max_tool = c.tools_sorted.first().map(|(_, v)| *v).unwrap_or(1);
            let total_tools_sum: u64 = c.tools_sorted.iter().map(|(_, count)| count).sum();

            let tool_name_width = c
                .tools_sorted
                .iter()
                .take(max_rows)
                .map(|(n, _)| n.len())
                .max()
                .unwrap_or(12)
                .max(12)
                .min(col_w.saturating_sub(40));

            let mut lines: Vec<Line<'static>> = Vec::new();
            lines.push(Line::from(vec![
                Span::styled("  Tool Calls  ", theme::section_title_style()),
                Span::styled(
                    format!("{} total", c.total_tools),
                    theme::stat_number_style(),
                ),
            ]));
            lines.push(Line::from(""));

            for (name, count) in c.tools_sorted.iter().take(max_rows) {
                let ratio = (*count as f64 / max_tool as f64).min(1.0);
                let pct = if total_tools_sum > 0 {
                    *count as f64 / total_tools_sum as f64 * 100.0
                } else {
                    0.0
                };
                let filled = (ratio * bar_width as f64).round() as usize;
                let empty = bar_width.saturating_sub(filled);
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("  {:<width$}  ", name, width = tool_name_width),
                        theme::label_style(),
                    ),
                    Span::styled("\u{2588}".repeat(filled), theme::bar_filled_style_green()),
                    Span::styled("\u{2591}".repeat(empty), theme::bar_empty_style()),
                    Span::styled(format!(" {:>5} ", count), theme::value_style()),
                    Span::styled(format!("{:>3.0}%", pct), theme::stat_secondary_style()),
                ]));
            }

            frame.render_widget(Paragraph::new(lines), rows[6]);
        }

        Ok(())
    }
}
