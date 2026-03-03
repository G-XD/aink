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
use super::common::{tab_bar, theme, time_filter::TimeFilter};
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
}

impl Default for TabHost {
    fn default() -> Self {
        Self {
            active_tab: DEFAULT_TAB,
            overview: Overview::new(),
            sessions: Home::default(),
            analysis: Analysis::new(),
            command_tx: None,
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
        ];

        frame.render_widget(
            Paragraph::new(Line::from(spans)).right_aligned(),
            first_line,
        );
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
        let suppress_tab = self.active_tab == 1
            && (self.sessions.is_in_detail_view() || self.sessions.is_popup_open());
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

        // Popup and detail view get first crack at keys
        if self.active_tab == 1
            && (self.sessions.is_in_detail_view() || self.sessions.is_popup_open())
        {
            let result = self.sessions.handle_key_event(key)?;
            if result.is_some() {
                return Ok(result);
            }
        }

        let suppress_keys = self.active_tab == 1
            && (self.sessions.is_in_detail_view() || self.sessions.is_popup_open());

        if !suppress_keys {
            match key.code {
                KeyCode::Char('q') => {
                    return Ok(Some(Action::Quit));
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
            return self.sessions.draw(frame, area);
        }

        let [tab_bar_area, content_area] =
            Layout::vertical([Constraint::Length(2), Constraint::Min(3)]).areas(area);

        tab_bar::render_tab_bar(frame, tab_bar_area, self.active_tab);
        self.render_summary(frame, tab_bar_area);

        match self.active_tab {
            0 => self.overview.draw(frame, content_area),
            1 => self.sessions.draw(frame, content_area),
            2 => self.analysis.draw(frame, content_area),
            _ => Ok(()),
        }
    }
}
