//! Time filter popup: self-contained state + rendering + key handling.

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Layout, Rect},
    prelude::*,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use super::theme;
use super::time_filter::TimeFilter;
use crate::utils::format::parse_date_ymd;

const PRESETS: &[(&str, PresetKind)] = &[
    ("Today", PresetKind::Today),
    ("Last 7 days", PresetKind::Last7Days),
    ("Last 30 days", PresetKind::Last30Days),
    ("All time", PresetKind::All),
    ("Custom range", PresetKind::Custom),
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum PresetKind {
    Today,
    Last7Days,
    Last30Days,
    All,
    Custom,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Presets,
    FromInput,
    ToInput,
}

pub struct PopupState {
    cursor: usize,
    focus: Focus,
    from_input: String,
    to_input: String,
}

impl PopupState {
    pub fn new(current: &TimeFilter) -> Self {
        let (cursor, from_input, to_input) = match current {
            TimeFilter::Today => (0, String::new(), String::new()),
            TimeFilter::Last7Days => (1, String::new(), String::new()),
            TimeFilter::Last30Days => (2, String::new(), String::new()),
            TimeFilter::All => (3, String::new(), String::new()),
            TimeFilter::Custom { from, to } => (4, from.clone(), to.clone()),
        };
        Self {
            cursor,
            focus: Focus::Presets,
            from_input,
            to_input,
        }
    }

    /// Returns Some(Some(filter)) to apply, Some(None) to cancel, None to stay open.
    pub fn handle_key(&mut self, key: KeyEvent) -> Option<Option<TimeFilter>> {
        match self.focus {
            Focus::Presets => self.handle_preset_key(key),
            Focus::FromInput | Focus::ToInput => self.handle_input_key(key),
        }
    }

    fn handle_preset_key(&mut self, key: KeyEvent) -> Option<Option<TimeFilter>> {
        match key.code {
            KeyCode::Esc => Some(None),
            KeyCode::Up | KeyCode::Char('k') => {
                if self.cursor > 0 {
                    self.cursor -= 1;
                }
                None
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.cursor < PRESETS.len() - 1 {
                    self.cursor += 1;
                }
                None
            }
            KeyCode::Enter => {
                if self.selected_kind() == PresetKind::Custom {
                    self.focus = Focus::FromInput;
                    None
                } else {
                    Some(Some(self.build_filter()))
                }
            }
            _ => None,
        }
    }

    fn handle_input_key(&mut self, key: KeyEvent) -> Option<Option<TimeFilter>> {
        match key.code {
            KeyCode::Esc => {
                self.focus = Focus::Presets;
                None
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = if self.focus == Focus::FromInput {
                    Focus::ToInput
                } else {
                    Focus::FromInput
                };
                None
            }
            KeyCode::Enter => {
                if self.custom_dates_valid() {
                    Some(Some(self.build_filter()))
                } else {
                    None
                }
            }
            KeyCode::Backspace => {
                self.active_buf_mut().pop();
                None
            }
            KeyCode::Char(c) if c.is_ascii_digit() || c == '-' => {
                let buf = self.active_buf_mut();
                if buf.len() < 10 {
                    buf.push(c);
                }
                None
            }
            _ => None,
        }
    }

    fn selected_kind(&self) -> PresetKind {
        PRESETS
            .get(self.cursor)
            .map(|p| p.1)
            .unwrap_or(PresetKind::All)
    }

    fn build_filter(&self) -> TimeFilter {
        match self.selected_kind() {
            PresetKind::Today => TimeFilter::Today,
            PresetKind::Last7Days => TimeFilter::Last7Days,
            PresetKind::Last30Days => TimeFilter::Last30Days,
            PresetKind::All => TimeFilter::All,
            PresetKind::Custom => TimeFilter::Custom {
                from: self.from_input.clone(),
                to: self.to_input.clone(),
            },
        }
    }

    fn active_buf_mut(&mut self) -> &mut String {
        match self.focus {
            Focus::FromInput => &mut self.from_input,
            Focus::ToInput => &mut self.to_input,
            Focus::Presets => &mut self.from_input,
        }
    }

    fn custom_dates_valid(&self) -> bool {
        parse_date_ymd(&self.from_input).is_some()
            && parse_date_ymd(&self.to_input).is_some()
            && self.from_input <= self.to_input
    }
}

pub fn render_popup(frame: &mut Frame, area: Rect, state: &PopupState) {
    let popup_width: u16 = 40;
    let popup_height: u16 = 15;

    let x = area.x + area.width.saturating_sub(popup_width) / 2;
    let y = area.y + area.height.saturating_sub(popup_height) / 2;
    let popup_area = Rect::new(
        x,
        y,
        popup_width.min(area.width),
        popup_height.min(area.height),
    );

    frame.render_widget(Clear, popup_area);

    let block = Block::default()
        .title(" Filter by Time ")
        .title_style(theme::section_title_style())
        .borders(Borders::ALL)
        .border_style(theme::separator_style());
    let inner = block.inner(popup_area);
    frame.render_widget(block, popup_area);

    let [presets_area, _gap, inputs_area, _gap2, buttons_area] = Layout::vertical([
        Constraint::Length(PRESETS.len() as u16),
        Constraint::Length(1),
        Constraint::Length(4),
        Constraint::Min(0),
        Constraint::Length(1),
    ])
    .areas(inner);

    // Presets
    let preset_lines: Vec<Line> = PRESETS
        .iter()
        .enumerate()
        .map(|(i, (label, _))| {
            let marker = if i == state.cursor {
                "\u{25cf} "
            } else {
                "\u{25cb} "
            };
            let style = if i == state.cursor && state.focus == Focus::Presets {
                theme::highlight_style()
            } else if i == state.cursor {
                theme::tab_active_style()
            } else {
                theme::body_style()
            };
            Line::from(Span::styled(format!("   {}{}", marker, label), style))
        })
        .collect();
    frame.render_widget(Paragraph::new(preset_lines), presets_area);

    // Date inputs
    let is_custom = state.selected_kind() == PresetKind::Custom;
    let input_style = if is_custom {
        theme::body_style()
    } else {
        theme::footer_style()
    };

    let from_border_style = if state.focus == Focus::FromInput {
        if !state.from_input.is_empty() && parse_date_ymd(&state.from_input).is_none() {
            Style::default().fg(Color::Rgb(255, 85, 110)) // red
        } else {
            theme::tab_active_style()
        }
    } else {
        input_style
    };

    let to_border_style = if state.focus == Focus::ToInput {
        if !state.to_input.is_empty() && parse_date_ymd(&state.to_input).is_none() {
            Style::default().fg(Color::Rgb(255, 85, 110))
        } else {
            theme::tab_active_style()
        }
    } else {
        input_style
    };

    let [from_area, to_area] =
        Layout::vertical([Constraint::Length(2), Constraint::Length(2)]).areas(inputs_area);

    let from_display = if state.from_input.is_empty() && is_custom {
        "YYYY-MM-DD".to_string()
    } else {
        state.from_input.clone()
    };
    let from_label = Line::from(vec![
        Span::styled("   From: ", theme::label_style()),
        Span::styled(from_display, from_border_style),
    ]);
    frame.render_widget(Paragraph::new(from_label), from_area);

    let to_display = if state.to_input.is_empty() && is_custom {
        "YYYY-MM-DD".to_string()
    } else {
        state.to_input.clone()
    };
    let to_label = Line::from(vec![
        Span::styled("   To:   ", theme::label_style()),
        Span::styled(to_display, to_border_style),
    ]);
    frame.render_widget(Paragraph::new(to_label), to_area);

    // Button hints
    let hint = if state.focus == Focus::Presets {
        Line::from(vec![
            Span::styled("   ", theme::body_style()),
            Span::styled("Enter", theme::footer_key_style()),
            Span::styled(" apply  ", theme::footer_style()),
            Span::styled("Esc", theme::footer_key_style()),
            Span::styled(" cancel", theme::footer_style()),
        ])
    } else {
        Line::from(vec![
            Span::styled("   ", theme::body_style()),
            Span::styled("Tab", theme::footer_key_style()),
            Span::styled(" switch  ", theme::footer_style()),
            Span::styled("Enter", theme::footer_key_style()),
            Span::styled(" apply  ", theme::footer_style()),
            Span::styled("Esc", theme::footer_key_style()),
            Span::styled(" back", theme::footer_style()),
        ])
    };
    frame.render_widget(Paragraph::new(hint), buttons_area);
}
