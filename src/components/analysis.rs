//! Analysis tab: four-quadrant data visualization.
//! Token Usage, Tool Distribution, Sessions by Project, Cost by Project.

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
use crate::utils::format::{format_cost, format_tokens};
use crate::utils::project_name;

use super::Component;

fn render_separator_h(frame: &mut Frame, area: Rect) {
    let sep = theme::SEP_DASH.repeat(area.width as usize);
    let line = Paragraph::new(Line::from(Span::styled(sep, theme::separator_style())));
    frame.render_widget(line, area);
}

fn render_separator_v(frame: &mut Frame, area: Rect) {
    let mut lines: Vec<Line<'static>> = Vec::new();
    for _ in 0..area.height {
        lines.push(Line::from(Span::styled(
            "\u{2502}",
            theme::separator_style(),
        )));
    }
    frame.render_widget(Paragraph::new(lines), area);
}

/// Render the quadrant content into a specific area.
fn render_token_usage(frame: &mut Frame, area: Rect, project_tokens: &[(String, u64)]) {
    let max_rows = area.height.saturating_sub(2) as usize;
    let col_w = area.width as usize;
    let bar_width = 16usize.min(col_w.saturating_sub(28));
    let max_tokens = project_tokens.first().map(|(_, t)| *t).unwrap_or(1);

    let name_width = project_tokens
        .iter()
        .take(max_rows)
        .map(|(n, _)| n.len())
        .max()
        .unwrap_or(12)
        .max(12)
        .min(col_w.saturating_sub(28));

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(Span::styled(
        "  Token Usage by Project",
        theme::section_title_style(),
    )));
    lines.push(Line::from(""));

    for (name, tokens) in project_tokens.iter().take(max_rows) {
        let ratio = (*tokens as f64 / max_tokens as f64).min(1.0);
        let filled = (ratio * bar_width as f64).round() as usize;
        let empty = bar_width.saturating_sub(filled);
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{:<width$}  ", name, width = name_width),
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

    frame.render_widget(Paragraph::new(lines), area);
}

fn render_tool_distribution(frame: &mut Frame, area: Rect, tools_sorted: &[(String, u64)]) {
    let max_rows = area.height.saturating_sub(2) as usize;
    let col_w = area.width as usize;
    let bar_width = 16usize.min(col_w.saturating_sub(26));
    let max_tool = tools_sorted.first().map(|(_, v)| *v).unwrap_or(1);
    let total_tools: u64 = tools_sorted.iter().map(|(_, c)| c).sum();

    let name_width = tools_sorted
        .iter()
        .take(max_rows)
        .map(|(n, _)| n.len())
        .max()
        .unwrap_or(12)
        .max(12)
        .min(col_w.saturating_sub(26));

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(Span::styled(
        "  Tool Distribution",
        theme::section_title_style(),
    )));
    lines.push(Line::from(""));

    for (name, count) in tools_sorted.iter().take(max_rows) {
        let ratio = (*count as f64 / max_tool as f64).min(1.0);
        let pct = if total_tools > 0 {
            *count as f64 / total_tools as f64 * 100.0
        } else {
            0.0
        };
        let filled = (ratio * bar_width as f64).round() as usize;
        let empty = bar_width.saturating_sub(filled);
        lines.push(Line::from(vec![
            Span::styled(
                format!("  {:<width$}  ", name, width = name_width),
                theme::label_style(),
            ),
            Span::styled("\u{2588}".repeat(filled), theme::bar_filled_style_green()),
            Span::styled("\u{2591}".repeat(empty), theme::bar_empty_style()),
            Span::styled(format!(" {:>5} ", count), theme::value_style()),
            Span::styled(format!("{:>3.0}%", pct), theme::stat_secondary_style()),
        ]));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn render_sessions_by_project(frame: &mut Frame, area: Rect, project_sessions: &[(String, u64)]) {
    let max_rows = area.height.saturating_sub(2) as usize;
    let col_w = area.width as usize;
    let bar_width = 16usize.min(col_w.saturating_sub(24));
    let max_sessions = project_sessions.first().map(|(_, s)| *s).unwrap_or(1);

    let name_width = project_sessions
        .iter()
        .take(max_rows)
        .map(|(n, _)| n.len())
        .max()
        .unwrap_or(12)
        .max(12)
        .min(col_w.saturating_sub(24));

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(Span::styled(
        "  Sessions by Project",
        theme::section_title_style(),
    )));
    lines.push(Line::from(""));

    for (name, count) in project_sessions.iter().take(max_rows) {
        let ratio = (*count as f64 / max_sessions as f64).min(1.0);
        let filled = (ratio * bar_width as f64).round() as usize;
        let empty = bar_width.saturating_sub(filled);
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{:<width$}  ", name, width = name_width),
                theme::value_style(),
            ),
            Span::styled("\u{2588}".repeat(filled), theme::bar_filled_style()),
            Span::styled("\u{2591}".repeat(empty), theme::bar_empty_style()),
            Span::styled(format!(" {:>5}", count), theme::stat_number_style()),
        ]));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

fn render_cost_by_project(frame: &mut Frame, area: Rect, project_costs: &[(String, f64)]) {
    let max_rows = area.height.saturating_sub(2) as usize;
    let col_w = area.width as usize;
    let bar_width = 12usize.min(col_w.saturating_sub(24));
    let max_cost = project_costs.first().map(|(_, c)| *c).unwrap_or(1.0);

    let name_width = project_costs
        .iter()
        .take(max_rows)
        .map(|(n, _)| n.len())
        .max()
        .unwrap_or(12)
        .max(12)
        .min(col_w.saturating_sub(24));

    let mut lines: Vec<Line<'static>> = Vec::new();
    lines.push(Line::from(Span::styled(
        "  Cost by Project",
        theme::section_title_style(),
    )));
    lines.push(Line::from(""));

    for (name, cost) in project_costs.iter().take(max_rows) {
        let ratio = if max_cost > 0.0 {
            (*cost / max_cost).min(1.0)
        } else {
            0.0
        };
        let filled = (ratio * bar_width as f64).round() as usize;
        let empty = bar_width.saturating_sub(filled);
        lines.push(Line::from(vec![
            Span::raw("  "),
            Span::styled(
                format!("{:<width$}  ", name, width = name_width),
                theme::value_style(),
            ),
            Span::styled("\u{2588}".repeat(filled), theme::bar_filled_style()),
            Span::styled("\u{2591}".repeat(empty), theme::bar_empty_style()),
            Span::styled(
                format!(" {:>7}", format_cost(*cost)),
                theme::stat_number_style(),
            ),
        ]));
    }

    frame.render_widget(Paragraph::new(lines), area);
}

/// Pre-computed aggregate data for rendering, updated only when transcripts change.
struct AnalysisCache {
    project_tokens: Vec<(String, u64)>,
    tools_sorted: Vec<(String, u64)>,
    project_sessions: Vec<(String, u64)>,
    project_costs: Vec<(String, f64)>,
}

#[derive(Default)]
pub struct Analysis {
    scroll: u16,
    cache: Option<AnalysisCache>,
}

impl Analysis {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_transcripts(&mut self, data: Arc<SessionList>) {
        self.scroll = 0;

        if data.is_empty() {
            self.cache = None;
            return;
        }

        let mut token_map: HashMap<String, u64> = HashMap::new();
        let mut cost_map: HashMap<String, f64> = HashMap::new();
        let mut tool_counts: HashMap<String, u64> = HashMap::new();
        let mut session_map: HashMap<String, u64> = HashMap::new();

        for (path, data) in data.iter() {
            let name = project_name::session_display_name(path, data, 20);
            let total = data.input_tokens + data.output_tokens;
            *token_map.entry(name.clone()).or_insert(0) += total;

            for (tool, count) in &data.tool_call_by_type {
                *tool_counts.entry(tool.clone()).or_insert(0) += count;
            }

            *session_map.entry(name.clone()).or_insert(0) += 1;

            *cost_map.entry(name).or_insert(0.0) += data.estimated_cost_usd;
        }

        let mut project_tokens: Vec<_> = token_map.into_iter().collect();
        project_tokens.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let mut tools_sorted: Vec<_> = tool_counts.into_iter().collect();
        tools_sorted.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let mut project_sessions: Vec<_> = session_map.into_iter().collect();
        project_sessions.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        let mut project_costs: Vec<_> = cost_map.into_iter().collect();
        project_costs.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
        });

        self.cache = Some(AnalysisCache {
            project_tokens,
            tools_sorted,
            project_sessions,
            project_costs,
        });
    }
}

impl Component for Analysis {
    fn handle_key_event(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> color_eyre::Result<Option<crate::action::Action>> {
        use crossterm::event::KeyCode;
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.scroll = self.scroll.saturating_sub(1);
                return Ok(Some(crate::action::Action::Render));
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.scroll = self.scroll.saturating_add(1);
                return Ok(Some(crate::action::Action::Render));
            }
            _ => {}
        }
        Ok(None)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        let Some(c) = &self.cache else {
            let msg = Paragraph::new("  No data yet. Switch to Sessions tab and load transcripts.")
                .style(theme::empty_msg_style());
            frame.render_widget(msg, area);
            return Ok(());
        };

        let wide = area.width >= 80;

        if wide {
            // ── Wide: 2x2 quadrant layout ──
            let rows = Layout::vertical([
                Constraint::Percentage(50),
                Constraint::Length(1),
                Constraint::Percentage(50),
            ])
            .split(area);

            let top_cols = Layout::horizontal([
                Constraint::Percentage(50),
                Constraint::Length(1),
                Constraint::Percentage(50),
            ])
            .split(rows[0]);

            render_token_usage(frame, top_cols[0], &c.project_tokens);
            render_separator_v(frame, top_cols[1]);
            render_tool_distribution(frame, top_cols[2], &c.tools_sorted);

            render_separator_h(frame, rows[1]);

            let bot_cols = Layout::horizontal([
                Constraint::Percentage(50),
                Constraint::Length(1),
                Constraint::Percentage(50),
            ])
            .split(rows[2]);

            render_sessions_by_project(frame, bot_cols[0], &c.project_sessions);
            render_separator_v(frame, bot_cols[1]);
            render_cost_by_project(frame, bot_cols[2], &c.project_costs);
        } else {
            // ── Narrow: vertical stack with scroll ──
            let mut lines: Vec<Line<'static>> = Vec::new();

            let bar_width: usize = 12;

            let session_name_width = c
                .project_tokens
                .iter()
                .take(8)
                .map(|(n, _)| n.len())
                .max()
                .unwrap_or(12)
                .max(12);

            // Token Usage
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "  Token Usage",
                theme::section_title_style(),
            )));
            lines.push(Line::from(""));

            let max_tokens = c.project_tokens.first().map(|(_, t)| *t).unwrap_or(1);
            for (name, tokens) in c.project_tokens.iter().take(8) {
                let ratio = (*tokens as f64 / max_tokens as f64).min(1.0);
                let filled = (ratio * bar_width as f64).round() as usize;
                let empty = bar_width.saturating_sub(filled);
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        format!("{:<width$}  ", name, width = session_name_width),
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

            lines.push(Line::from(""));
            let sep = theme::SEP_DASH.repeat(area.width.saturating_sub(4) as usize);
            lines.push(Line::from(Span::styled(
                format!("  {}", sep),
                theme::separator_style(),
            )));
            lines.push(Line::from(""));

            // Sessions by Project
            let max_sessions = c.project_sessions.first().map(|(_, s)| *s).unwrap_or(1);
            lines.push(Line::from(Span::styled(
                "  Sessions by Project",
                theme::section_title_style(),
            )));
            lines.push(Line::from(""));

            for (name, count) in c.project_sessions.iter().take(8) {
                let ratio = (*count as f64 / max_sessions as f64).min(1.0);
                let filled = (ratio * bar_width as f64).round() as usize;
                let empty = bar_width.saturating_sub(filled);
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        format!("{:<width$}  ", name, width = session_name_width),
                        theme::value_style(),
                    ),
                    Span::styled("\u{2588}".repeat(filled), theme::bar_filled_style()),
                    Span::styled("\u{2591}".repeat(empty), theme::bar_empty_style()),
                    Span::styled(format!(" {:>5}", count), theme::stat_number_style()),
                ]));
            }

            lines.push(Line::from(""));
            let sep = theme::SEP_DASH.repeat(area.width.saturating_sub(4) as usize);
            lines.push(Line::from(Span::styled(
                format!("  {}", sep),
                theme::separator_style(),
            )));
            lines.push(Line::from(""));

            // Tool Distribution
            lines.push(Line::from(Span::styled(
                "  Tool Distribution",
                theme::section_title_style(),
            )));
            lines.push(Line::from(""));

            let total_tools: u64 = c.tools_sorted.iter().map(|(_, count)| count).sum();
            let max_tool = c.tools_sorted.first().map(|(_, v)| *v).unwrap_or(1);
            let tool_name_width = c
                .tools_sorted
                .iter()
                .take(8)
                .map(|(n, _)| n.len())
                .max()
                .unwrap_or(12)
                .max(12);

            for (name, count) in c.tools_sorted.iter().take(8) {
                let ratio = (*count as f64 / max_tool as f64).min(1.0);
                let pct = if total_tools > 0 {
                    *count as f64 / total_tools as f64 * 100.0
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

            lines.push(Line::from(""));
            let sep = theme::SEP_DASH.repeat(area.width.saturating_sub(4) as usize);
            lines.push(Line::from(Span::styled(
                format!("  {}", sep),
                theme::separator_style(),
            )));
            lines.push(Line::from(""));

            // Cost by Project
            lines.push(Line::from(Span::styled(
                "  Cost by Project",
                theme::section_title_style(),
            )));
            lines.push(Line::from(""));

            let max_cost = c
                .project_costs
                .first()
                .map(|(_, cost)| *cost)
                .unwrap_or(1.0);
            for (name, cost) in c.project_costs.iter().take(8) {
                let ratio = if max_cost > 0.0 {
                    (*cost / max_cost).min(1.0)
                } else {
                    0.0
                };
                let filled = (ratio * bar_width as f64).round() as usize;
                let empty = bar_width.saturating_sub(filled);
                lines.push(Line::from(vec![
                    Span::raw("  "),
                    Span::styled(
                        format!("{:<width$}  ", name, width = session_name_width),
                        theme::value_style(),
                    ),
                    Span::styled("\u{2588}".repeat(filled), theme::bar_filled_style()),
                    Span::styled("\u{2591}".repeat(empty), theme::bar_empty_style()),
                    Span::styled(
                        format!(" {:>7}", format_cost(*cost)),
                        theme::stat_number_style(),
                    ),
                ]));
            }

            let para = Paragraph::new(lines).scroll((self.scroll, 0));
            frame.render_widget(para, area);
        }

        Ok(())
    }
}
