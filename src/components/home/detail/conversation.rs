//! Conversation tab: turn-by-turn display with expand/collapse and cursor navigation.
//!
//! Uses windowed rendering: only builds `Line` objects for turns near the
//! visible cursor, keeping frame cost O(viewport) instead of O(total_turns).

use std::collections::HashSet;

use ratatui::prelude::*;
use ratatui::text::Line;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::collector::transcript::{ConversationRole, ConversationTurn};
use crate::components::common::theme;
use crate::utils::format::format_conversation_time;

const MAX_PREVIEW_LINES: usize = 3;

/// Average collapsed-turn height (header + sep + tool summary + preview + blank).
const AVG_TURN_LINES: usize = 6;

pub const SECTION_MSG_PREFIX: &str = "msg_";

/// Soft-wrap text at word boundaries to fit within `max_width` display columns.
/// Falls back to character-level breaking for words longer than `max_width`.
/// Uses unicode display width so CJK characters (2 columns each) wrap correctly.
fn wrap_text(text: &str, max_width: usize) -> Vec<String> {
    if max_width == 0 {
        return vec![text.to_string()];
    }
    if UnicodeWidthStr::width(text) <= max_width {
        return vec![text.to_string()];
    }

    let mut result: Vec<String> = Vec::new();
    let mut line = String::new();
    let mut line_width: usize = 0;

    for word in text.split_whitespace() {
        let word_w = UnicodeWidthStr::width(word);

        if line_width == 0 {
            if word_w <= max_width {
                line.push_str(word);
                line_width = word_w;
            } else {
                for ch in word.chars() {
                    let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
                    if line_width + cw > max_width {
                        result.push(std::mem::take(&mut line));
                        line_width = 0;
                    }
                    line.push(ch);
                    line_width += cw;
                }
            }
        } else if line_width + 1 + word_w <= max_width {
            line.push(' ');
            line.push_str(word);
            line_width += 1 + word_w;
        } else {
            result.push(std::mem::take(&mut line));
            line_width = 0;
            if word_w <= max_width {
                line.push_str(word);
                line_width = word_w;
            } else {
                for ch in word.chars() {
                    let cw = UnicodeWidthChar::width(ch).unwrap_or(0);
                    if line_width + cw > max_width {
                        result.push(std::mem::take(&mut line));
                        line_width = 0;
                    }
                    line.push(ch);
                    line_width += cw;
                }
            }
        }
    }
    if !line.is_empty() {
        result.push(line);
    }
    if result.is_empty() {
        result.push(String::new());
    }
    result
}

/// Estimate how many rendered (wrapped) lines a single source line produces.
fn estimate_wrapped(text: &str, usable_w: usize) -> usize {
    if usable_w == 0 {
        return 1;
    }
    let n = UnicodeWidthStr::width(text);
    n.div_ceil(usable_w).max(1)
}

/// Estimate how many rendered lines a turn produces without allocating Line objects.
fn estimate_turn_lines(turn: &ConversationTurn, is_expanded: bool, usable_w: usize) -> usize {
    let mut count = 2; // header + separator

    if !turn.tool_calls.is_empty() {
        count += 1; // summary line
        if is_expanded {
            let tc_width = usable_w.saturating_sub(12);
            for tc in &turn.tool_calls {
                count += estimate_wrapped(&tc.summary, tc_width).max(1);
            }
        }
    }

    let total_rendered: usize = turn
        .content
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .map(|l| estimate_wrapped(l, usable_w))
        .sum();

    if is_expanded || total_rendered <= MAX_PREVIEW_LINES {
        count += total_rendered;
    } else {
        count += MAX_PREVIEW_LINES + 1; // preview + "[+N more]"
    }

    count + 1 // trailing blank
}

/// Build conversation content with cursor-based navigation.
///
/// Only materialises `Line` objects for a window of turns around `cursor`,
/// keeping per-frame cost proportional to the viewport rather than the full
/// conversation length.
///
/// Returns `(lines, cursor_line_offset)` for auto-scrolling.
pub fn detail_conversation_content(
    conversation: Option<&[ConversationTurn]>,
    expanded_sections: &HashSet<String>,
    cursor: usize,
    width: u16,
    viewport_height: u16,
) -> (Vec<Line<'static>>, usize) {
    let w = width as usize;
    let usable = w.saturating_sub(3); // 2-char left margin + 1-char scrollbar

    let Some(turns) = conversation else {
        return (
            vec![Line::from(Span::styled(
                "  (could not load conversation)",
                theme::fold_style(),
            ))],
            0,
        );
    };

    let total = turns.len();
    if total == 0 {
        return (Vec::new(), 0);
    }

    // Determine the window of turns to render.
    let margin_turns = (viewport_height as usize / AVG_TURN_LINES).max(5) * 2;
    let win_start = cursor.saturating_sub(margin_turns);
    let win_end = (cursor + margin_turns + 1).min(total);

    // Count lines for turns before the window (cheap — no allocations).
    let lines_before: usize = turns[..win_start]
        .iter()
        .enumerate()
        .map(|(idx, turn)| {
            let expanded = expanded_sections.contains(&format!("{}{}", SECTION_MSG_PREFIX, idx));
            estimate_turn_lines(turn, expanded, usable)
        })
        .sum();

    // Count lines for turns after the window.
    let lines_after: usize = turns[win_end..]
        .iter()
        .enumerate()
        .map(|(i, turn)| {
            let idx = win_end + i;
            let expanded = expanded_sections.contains(&format!("{}{}", SECTION_MSG_PREFIX, idx));
            estimate_turn_lines(turn, expanded, usable)
        })
        .sum();

    // Build placeholder lines for the region above the window so that
    // scroll offsets and the scrollbar remain correct.
    let capacity = lines_before + lines_after + margin_turns * 2 * (AVG_TURN_LINES + 2);
    let mut lines: Vec<Line<'static>> = Vec::with_capacity(capacity);

    for _ in 0..lines_before {
        lines.push(Line::from(""));
    }

    let mut cursor_line: usize = 0;

    // Render the visible window of turns.
    for (turn_idx, turn) in turns.iter().enumerate().take(win_end).skip(win_start) {
        let is_selected = turn_idx == cursor;
        let is_expanded =
            expanded_sections.contains(&format!("{}{}", SECTION_MSG_PREFIX, turn_idx));

        if is_selected {
            cursor_line = lines.len();
        }

        // ── Header: selection bar + role + turn counter ──
        let (role_label, role_style) = match turn.role {
            ConversationRole::User => ("User", theme::conv_user_style()),
            ConversationRole::Assistant => ("Assistant", theme::conv_assistant_style()),
        };

        let bar = if is_selected { "│" } else { " " };
        let bar_style = if is_selected {
            theme::selection_bar_style()
        } else {
            theme::body_style()
        };

        let time_str = format_conversation_time(turn.created_at.as_deref());
        let counter = format!("[{}/{}]", turn_idx + 1, total);
        let padding =
            w.saturating_sub(4 + role_label.len() + 1 + time_str.len() + 1 + counter.len());

        lines.push(Line::from(vec![
            Span::styled(format!("{} ", bar), bar_style),
            Span::styled(role_label, role_style),
            Span::raw(" "),
            Span::styled(time_str, theme::fold_style()),
            Span::raw(" ".repeat(padding)),
            Span::styled(counter, theme::fold_style()),
        ]));

        // Separator under header
        lines.push(Line::from(Span::styled(
            format!("  {}", theme::SEP_DASH.repeat(w.saturating_sub(4))),
            theme::separator_style(),
        )));

        // ── Tool calls ──
        if !turn.tool_calls.is_empty() {
            let tool_names: Vec<&str> = turn.tool_calls.iter().map(|tc| tc.name.as_str()).collect();
            let icon = if is_expanded { "\u{25be}" } else { "\u{25b8}" };
            let preview = if tool_names.len() <= 4 {
                tool_names.join(", ")
            } else {
                let first = tool_names[..3].join(", ");
                format!("{}, +{} more", first, tool_names.len() - 3)
            };
            lines.push(Line::from(Span::styled(
                format!(
                    "  {} {} tool calls ({})",
                    icon,
                    turn.tool_calls.len(),
                    preview
                ),
                theme::fold_style(),
            )));

            if is_expanded {
                for tc in &turn.tool_calls {
                    if tc.summary.is_empty() {
                        lines.push(Line::from(Span::styled(
                            format!("      [{}]", tc.name),
                            theme::fold_style(),
                        )));
                    } else {
                        let wrap_width = usable.saturating_sub(12);
                        let wrapped = wrap_text(&tc.summary, wrap_width);
                        let first_max = usable.saturating_sub(10 + tc.name.len());
                        let first_chunk = wrap_text(wrapped[0].as_str(), first_max);
                        lines.push(Line::from(vec![
                            Span::styled(format!("      [{}]  ", tc.name), theme::fold_style()),
                            Span::styled(first_chunk[0].clone(), theme::value_style()),
                        ]));
                        for line in first_chunk.iter().skip(1).chain(wrapped.iter().skip(1)) {
                            lines.push(Line::from(Span::styled(
                                format!("            {}", line),
                                theme::value_style(),
                            )));
                        }
                    }
                }
            }
        }

        // ── Text content (word-wrapped) ──
        let source_lines: Vec<&str> = turn
            .content
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();

        let mut rendered: Vec<String> = Vec::new();
        for text in &source_lines {
            rendered.extend(wrap_text(text, usable));
        }

        if is_expanded || rendered.len() <= MAX_PREVIEW_LINES {
            for text in &rendered {
                lines.push(Line::from(Span::styled(
                    format!("  {}", text),
                    theme::value_style(),
                )));
            }
        } else {
            for text in rendered.iter().take(MAX_PREVIEW_LINES) {
                lines.push(Line::from(Span::styled(
                    format!("  {}", text),
                    theme::value_style(),
                )));
            }
            let remaining = rendered.len() - MAX_PREVIEW_LINES;
            lines.push(Line::from(Span::styled(
                format!("  [+{} more lines]", remaining),
                theme::fold_style(),
            )));
        }

        lines.push(Line::from(""));
    }

    // Placeholder lines for the region below the window.
    for _ in 0..lines_after {
        lines.push(Line::from(""));
    }

    (lines, cursor_line)
}
