//! TabHost: top-level component owning Overview, Sessions, Analysis tabs.
//! Renders tab bar and delegates to the active tab.

use std::sync::Arc;

use ratatui::{
    layout::{Constraint, Layout, Rect},
    prelude::*,
    text::Span,
    widgets::Paragraph,
};
use tokio::sync::mpsc::UnboundedSender;

use super::Component;
use super::common::{
    tab_bar, theme,
    time_filter::TimeFilter,
    time_filter_popup::{self, PopupState},
};
use crate::{
    action::Action,
    collector::transcript::SessionList,
    config::Config,
    utils::format::{format_cost, format_tokens},
};

use super::analysis::Analysis;
use super::home::Home;
use super::overview::Overview;

const TAB_COUNT: usize = 3;
const DEFAULT_TAB: usize = 1; // Sessions

pub struct TabHost {
    active_tab: usize,
    overview: Overview,
    sessions: Home,
    analysis: Analysis,
    command_tx: Option<UnboundedSender<Action>>,
    filter_popup: Option<PopupState>,
    /// Export status message (success or failure)
    export_message: Option<ExportMessage>,
}

struct ExportMessage {
    text: String,
    is_success: bool,
    /// Message creation time (used for auto-clearing)
    created_at: std::time::Instant,
}

impl Default for TabHost {
    fn default() -> Self {
        Self {
            active_tab: DEFAULT_TAB,
            overview: Overview::new(),
            sessions: Home::default(),
            analysis: Analysis::new(),
            command_tx: None,
            filter_popup: None,
            export_message: None,
        }
    }
}

impl TabHost {
    pub fn new(initial_filter: TimeFilter) -> Self {
        Self {
            active_tab: DEFAULT_TAB,
            overview: Overview::new(),
            sessions: Home::new(initial_filter),
            analysis: Analysis::new(),
            command_tx: None,
            filter_popup: None,
            export_message: None,
        }
    }

    fn sync_data_to_tabs(&mut self) {
        let data: Arc<SessionList> = Arc::new(self.sessions.transcripts().to_vec());
        self.overview.set_transcripts(Arc::clone(&data));
        self.analysis.set_transcripts(data);
    }

    fn switch_tab(&mut self, index: usize) {
        if index < TAB_COUNT {
            self.active_tab = index;
            self.sync_data_to_tabs();
        }
    }

    fn render_summary(&self, frame: &mut Frame, area: Rect) {
        let transcripts = self.sessions.transcripts();
        if transcripts.is_empty() && !self.sessions.is_filter_active() {
            return;
        }

        let session_count = transcripts.len();
        let (total_tokens, total_cost) =
            transcripts
                .iter()
                .fold((0u64, 0.0f64), |(tokens, cost), (_, t)| {
                    (
                        tokens + t.input_tokens + t.output_tokens,
                        cost + t.estimated_cost_usd,
                    )
                });

        let [first_line, _] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);

        let session_label = if self.sessions.is_filter_active() {
            let total = self.sessions.all_transcript_count();
            format!("{}/{} ", session_count, total)
        } else {
            format!("{} ", session_count)
        };

        // Filter label (always shown)
        let filter_label_text = self.sessions.filter_label();
        let filter_style = if self.sessions.is_filter_active() {
            theme::tab_active_style()
        } else {
            theme::footer_style()
        };

        let spans = vec![
            Span::styled(session_label, theme::stat_number_style()),
            Span::styled("sessions · ", theme::footer_style()),
            Span::styled(
                format!("{} ", format_cost(total_cost)),
                theme::cost_style(total_cost),
            ),
            Span::styled("· ", theme::footer_style()),
            Span::styled(
                format!("{} ", format_tokens(total_tokens)),
                theme::stat_number_style(),
            ),
            Span::styled("tokens ", theme::footer_style()),
            Span::styled("[", theme::footer_style()),
            Span::styled(filter_label_text, filter_style),
            Span::styled("]  ", theme::footer_style()),
        ];

        frame.render_widget(
            Paragraph::new(Line::from(spans)).right_aligned(),
            first_line,
        );
    }

    /// Render export feedback message at the bottom of the TUI
    fn render_export_message(&self, frame: &mut Frame, area: Rect) {
        if let Some(ref msg) = self.export_message {
            let style = if msg.is_success {
                Style::default().fg(Color::Green)
            } else {
                Style::default().fg(Color::Red)
            };

            let para = Paragraph::new(msg.text.as_str())
                .style(style)
                .alignment(Alignment::Center);

            frame.render_widget(para, area);
        }
    }
}

impl Component for TabHost {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> color_eyre::Result<()> {
        self.command_tx = Some(tx.clone());
        self.sessions.register_action_handler(tx)?;
        Ok(())
    }

    fn register_config_handler(&mut self, config: Config) -> color_eyre::Result<()> {
        self.sessions.register_config_handler(config)?;
        Ok(())
    }

    fn init(&mut self, area: Size) -> color_eyre::Result<()> {
        self.sessions.init(area)?;
        self.sync_data_to_tabs();
        Ok(())
    }

    fn update(&mut self, action: Action) -> color_eyre::Result<Option<Action>> {
        let suppress_tab = self.filter_popup.is_some()
            || (self.active_tab == 1 && self.sessions.is_in_detail_view());
        match &action {
            Action::TabNext => {
                if suppress_tab {
                    return Ok(None);
                }
                self.switch_tab((self.active_tab + 1) % TAB_COUNT);
                Ok(Some(Action::Render))
            }
            Action::TabPrev => {
                if suppress_tab {
                    return Ok(None);
                }
                self.switch_tab((self.active_tab + TAB_COUNT - 1) % TAB_COUNT);
                Ok(Some(Action::Render))
            }
            Action::TabSelect(i) => {
                self.switch_tab(*i);
                Ok(Some(Action::Render))
            }
            Action::RefreshTranscripts => {
                let result = self.sessions.update(action)?;
                Ok(result.or(Some(Action::Render)))
            }
            Action::TranscriptsLoaded => {
                let result = self.sessions.update(action)?;
                self.sync_data_to_tabs();
                Ok(result.or(Some(Action::Render)))
            }
            Action::ExportSession(index) => {
                // Get session data
                let transcripts = self.sessions.transcripts();
                if *index >= transcripts.len() {
                    // Invalid index, ignore
                    return Ok(None);
                }

                let (path, data) = &transcripts[*index];

                // Get conversation data
                let conversation = self.sessions.get_conversation(*index);

                // Execute export
                match crate::exporter::export_session(path, data, conversation.as_deref()) {
                    Ok(output_path) => {
                        let msg = format!("✓ Exported to: {}", output_path.display());
                        Ok(Some(Action::ExportComplete(Ok(msg))))
                    }
                    Err(e) => {
                        let msg = format!("✗ Export failed: {}", e);
                        Ok(Some(Action::ExportComplete(Err(msg))))
                    }
                }
            }
            Action::ExportComplete(result) => {
                match result {
                    Ok(msg) => {
                        self.export_message = Some(ExportMessage {
                            text: msg.clone(),
                            is_success: true,
                            created_at: std::time::Instant::now(),
                        });
                    }
                    Err(msg) => {
                        self.export_message = Some(ExportMessage {
                            text: msg.clone(),
                            is_success: false,
                            created_at: std::time::Instant::now(),
                        });
                    }
                }
                Ok(Some(Action::Render))
            }
            Action::Tick => {
                // Clear expired export messages
                if let Some(ref msg) = self.export_message {
                    let duration = if msg.is_success { 3 } else { 5 };
                    if msg.created_at.elapsed().as_secs() >= duration {
                        self.export_message = None;
                        return Ok(Some(Action::Render));
                    }
                }
                // Pass Tick to active tab
                match self.active_tab {
                    0 => self.overview.update(action),
                    1 => self.sessions.update(action),
                    2 => self.analysis.update(action),
                    _ => Ok(None),
                }
            }
            _ => match self.active_tab {
                0 => self.overview.update(action),
                1 => self.sessions.update(action),
                2 => self.analysis.update(action),
                _ => Ok(None),
            },
        }
    }

    fn handle_key_event(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> color_eyre::Result<Option<Action>> {
        use crossterm::event::KeyCode;

        // Filter popup gets first crack at all keys
        if let Some(ref mut popup) = self.filter_popup {
            if let Some(result) = popup.handle_key(key) {
                match result {
                    Some(filter) => {
                        self.filter_popup = None;
                        self.sessions.set_time_filter(filter);
                        self.sync_data_to_tabs();
                    }
                    None => {
                        self.filter_popup = None;
                    }
                }
            }
            return Ok(Some(Action::Render));
        }

        // Detail view gets first crack at keys
        if self.active_tab == 1 && self.sessions.is_in_detail_view() {
            let result = self.sessions.handle_key_event(key)?;
            if result.is_some() {
                return Ok(result);
            }
        }

        let suppress_keys = self.active_tab == 1 && self.sessions.is_in_detail_view();

        if !suppress_keys {
            match key.code {
                KeyCode::Char('q') => {
                    return Ok(Some(Action::Quit));
                }
                KeyCode::Char('f') => {
                    self.filter_popup = Some(PopupState::new(self.sessions.time_filter()));
                    return Ok(Some(Action::Render));
                }
                KeyCode::Char('O') => {
                    self.switch_tab(0);
                    return Ok(Some(Action::Render));
                }
                KeyCode::Char('S') => {
                    self.switch_tab(1);
                    return Ok(Some(Action::Render));
                }
                KeyCode::Char('A') => {
                    self.switch_tab(2);
                    return Ok(Some(Action::Render));
                }
                _ => {}
            }
        }

        if suppress_keys {
            return Ok(None);
        }
        match self.active_tab {
            0 => self.overview.handle_key_event(key),
            1 => self.sessions.handle_key_event(key),
            2 => self.analysis.handle_key_event(key),
            _ => Ok(None),
        }
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        if self.active_tab == 1 && self.sessions.is_in_detail_view() {
            // In detail view, we need to handle export message separately
            if self.export_message.is_some() {
                let [main_area, bottom_area] =
                    Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).areas(area);
                self.sessions.draw(frame, main_area)?;
                self.render_export_message(frame, bottom_area);
                return Ok(());
            }
            return self.sessions.draw(frame, area);
        }

        // Split layout: tab bar, content, and optional export message at bottom
        let (tab_bar_area, content_area, bottom_area) = if self.export_message.is_some() {
            let [tab_bar, rest] =
                Layout::vertical([Constraint::Length(2), Constraint::Min(4)]).areas(area);
            let [content, bottom] =
                Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).areas(rest);
            (tab_bar, content, Some(bottom))
        } else {
            let [tab_bar, content] =
                Layout::vertical([Constraint::Length(2), Constraint::Min(3)]).areas(area);
            (tab_bar, content, None)
        };

        tab_bar::render_tab_bar(frame, tab_bar_area, self.active_tab);
        self.render_summary(frame, tab_bar_area);

        let result = match self.active_tab {
            0 => self.overview.draw(frame, content_area),
            1 => self.sessions.draw(frame, content_area),
            2 => self.analysis.draw(frame, content_area),
            _ => Ok(()),
        };

        // Render export message at the bottom if present
        if let Some(bottom) = bottom_area {
            self.render_export_message(frame, bottom);
        }

        // Popup overlay (rendered on top of any tab)
        if let Some(ref popup) = self.filter_popup {
            time_filter_popup::render_popup(frame, area, popup);
        }

        result
    }
}
