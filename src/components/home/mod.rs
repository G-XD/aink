//! Sessions tab: transcript list + detail view.
//!
//! Transcript loading runs on a background thread so the UI renders
//! immediately with a spinner.  When loading finishes, a
//! [`TranscriptsLoaded`](crate::action::Action::TranscriptsLoaded) action
//! triggers data sync across tabs.

mod detail;
pub(crate) mod table;
mod view;

use std::sync::Arc;

use ratatui::{
    layout::{Constraint, Layout, Rect},
    prelude::*,
    widgets::{Paragraph, Table, TableState},
};
use tokio::sync::mpsc::UnboundedSender;

use super::Component;
use crate::{
    action::Action,
    collector::{
        source::{self, TranscriptSource},
        transcript::{ConversationTurn, SessionList, TranscriptData},
    },
    config::Config,
    utils::project_name,
};

use crate::components::common::{footer, theme};
use table::{
    COLUMN_WIDTHS, selection_next, selection_previous, set_selection, table_header, table_row,
};
use view::{DetailState, DetailTab, ExpandState, SortColumn, SortState, View};

const SPINNER_CHARS: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

pub struct Home {
    command_tx: Option<UnboundedSender<Action>>,
    config: Config,
    sources: Arc<Vec<Box<dyn TranscriptSource>>>,
    transcripts: SessionList,
    display_names: Vec<String>,
    table_state: TableState,
    view: View,
    sort: SortState,
    expand_state: ExpandState,
    detail_conversation: Option<Vec<ConversationTurn>>,
    loading: bool,
    load_rx: Option<std::sync::mpsc::Receiver<SessionList>>,
    spinner_tick: usize,
}

impl Default for Home {
    fn default() -> Self {
        Self {
            command_tx: None,
            config: Config::default(),
            sources: Arc::new(Vec::new()),
            transcripts: Vec::new(),
            display_names: Vec::new(),
            table_state: TableState::default(),
            view: View::List,
            sort: SortState::default(),
            expand_state: ExpandState::default(),
            detail_conversation: None,
            loading: false,
            load_rx: None,
            spinner_tick: 0,
        }
    }
}

impl Home {
    pub fn new() -> Self {
        Self::default()
    }

    fn start_loading(&mut self) {
        self.loading = true;
        self.spinner_tick = 0;

        let (tx, rx) = std::sync::mpsc::channel();
        self.load_rx = Some(rx);

        let sources = Arc::clone(&self.sources);
        let config = self.config.clone();
        let action_tx = self.command_tx.clone();

        std::thread::spawn(move || {
            let results = run_load(&sources, &config);
            let _ = tx.send(results);
            if let Some(atx) = action_tx {
                let _ = atx.send(Action::TranscriptsLoaded);
            }
        });
    }

    fn finish_loading(&mut self, data: SessionList) {
        self.transcripts = data;
        self.rebuild_display_names();
        self.apply_sort();
        self.view = View::List;
        if !self.transcripts.is_empty() {
            set_selection(&mut self.table_state, Some(0));
        }
        self.loading = false;
        self.load_rx = None;
    }

    fn rebuild_display_names(&mut self) {
        self.display_names = self
            .transcripts
            .iter()
            .map(|(path, data)| {
                project_name::session_display_name(path, data, table::SESSION_NAME_MAX_LEN)
            })
            .collect();
    }

    fn apply_sort(&mut self) {
        let col = self.sort.column;
        let asc = self.sort.ascending;
        self.transcripts.sort_by(|a, b| {
            let cmp = match col {
                SortColumn::Input => a.1.input_tokens.cmp(&b.1.input_tokens),
                SortColumn::Output => a.1.output_tokens.cmp(&b.1.output_tokens),
                SortColumn::Total => {
                    let ta = a.1.input_tokens + a.1.output_tokens;
                    let tb = b.1.input_tokens + b.1.output_tokens;
                    ta.cmp(&tb)
                }
                SortColumn::Active => {
                    let sa =
                        a.1.end_time
                            .as_deref()
                            .or(a.1.start_time.as_deref())
                            .unwrap_or("");
                    let sb =
                        b.1.end_time
                            .as_deref()
                            .or(b.1.start_time.as_deref())
                            .unwrap_or("");
                    sa.cmp(sb)
                }
                SortColumn::Cost => {
                    a.1.estimated_cost_usd
                        .partial_cmp(&b.1.estimated_cost_usd)
                        .unwrap_or(std::cmp::Ordering::Equal)
                }
            };
            if asc { cmp } else { cmp.reverse() }
        });
        self.rebuild_display_names();
    }

    pub fn transcripts(&self) -> &SessionList {
        &self.transcripts
    }

    fn source_for(&self, data: &TranscriptData) -> Option<&dyn TranscriptSource> {
        self.sources
            .iter()
            .find(|s| s.kind() == data.source)
            .map(|s| s.as_ref())
    }

    fn table_next(&mut self) {
        let len = self.transcripts.len();
        let next = selection_next(self.table_state.selected(), len);
        set_selection(&mut self.table_state, next);
    }

    fn table_previous(&mut self) {
        let len = self.transcripts.len();
        let prev = selection_previous(self.table_state.selected(), len);
        set_selection(&mut self.table_state, prev);
    }

    fn go_back_to_list(&mut self) {
        self.view = View::List;
        self.detail_conversation = None;
    }

    pub fn is_in_detail_view(&self) -> bool {
        matches!(self.view, View::Detail(_))
    }

    fn enter_detail(&mut self, index: usize) {
        let (path, data) = &self.transcripts[index];
        self.detail_conversation = self
            .source_for(data)
            .and_then(|s| s.parse_conversation(path).ok());
        self.view = View::Detail(DetailState::new(index));
    }

    // ── Key handling per view mode ──────────────────────────────────────

    fn handle_list_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        use crossterm::event::KeyCode;
        match key.code {
            KeyCode::Down | KeyCode::Char('j') => {
                self.table_next();
                Some(Action::Render)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.table_previous();
                Some(Action::Render)
            }
            KeyCode::Right | KeyCode::Char('l') => {
                if let Some(i) = self.table_state.selected()
                    && i < self.transcripts.len()
                    && self.transcripts[i].1.models.len() > 1
                    && !self.expand_state.is_expanded(i)
                {
                    self.expand_state.toggle(i);
                    return Some(Action::Render);
                }
                None
            }
            KeyCode::Left | KeyCode::Char('h') => {
                if let Some(i) = self.table_state.selected()
                    && self.expand_state.is_expanded(i)
                {
                    self.expand_state.toggle(i);
                    return Some(Action::Render);
                }
                None
            }
            KeyCode::Char('>') | KeyCode::Char('.') => {
                if let Some(next) = self.sort.column.next() {
                    self.sort.column = next;
                    self.apply_sort();
                    self.expand_state = ExpandState::default();
                    set_selection(&mut self.table_state, Some(0));
                    Some(Action::Render)
                } else {
                    None
                }
            }
            KeyCode::Char('<') | KeyCode::Char(',') => {
                if let Some(prev) = self.sort.column.prev() {
                    self.sort.column = prev;
                    self.apply_sort();
                    self.expand_state = ExpandState::default();
                    set_selection(&mut self.table_state, Some(0));
                    Some(Action::Render)
                } else {
                    None
                }
            }
            KeyCode::Char('s') => {
                self.sort.ascending = !self.sort.ascending;
                self.apply_sort();
                self.expand_state = ExpandState::default();
                set_selection(&mut self.table_state, Some(0));
                Some(Action::Render)
            }
            KeyCode::Enter => {
                if let Some(i) = self.table_state.selected()
                    && i < self.transcripts.len()
                {
                    self.enter_detail(i);
                    return Some(Action::Render);
                }
                None
            }
            _ => None,
        }
    }

    fn handle_detail_key(&mut self, key: crossterm::event::KeyEvent) -> Option<Action> {
        use crossterm::event::KeyCode;

        let View::Detail(ref mut ds) = self.view else {
            return None;
        };

        let conv_len = self
            .detail_conversation
            .as_ref()
            .map(|c| c.len())
            .unwrap_or(0);

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.go_back_to_list();
                Some(Action::Render)
            }
            KeyCode::Char('S') => {
                ds.active_tab = DetailTab::Stats;
                Some(Action::Render)
            }
            KeyCode::Char('C') => {
                ds.active_tab = DetailTab::Conversation;
                Some(Action::Render)
            }
            KeyCode::Char('F') => {
                ds.active_tab = DetailTab::Files;
                Some(Action::Render)
            }
            KeyCode::Tab => {
                ds.active_tab = ds.active_tab.next();
                Some(Action::Render)
            }
            KeyCode::BackTab => {
                ds.active_tab = ds.active_tab.prev();
                Some(Action::Render)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if ds.active_tab == DetailTab::Conversation && conv_len > 0 && ds.conv_cursor > 0 {
                    ds.conv_cursor = ds.conv_cursor.saturating_sub(1);
                } else {
                    let s = ds.current_scroll();
                    ds.set_current_scroll(s.saturating_sub(1));
                }
                Some(Action::Render)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if ds.active_tab == DetailTab::Conversation
                    && conv_len > 0
                    && ds.conv_cursor < conv_len.saturating_sub(1)
                {
                    ds.conv_cursor = (ds.conv_cursor + 1).min(conv_len - 1);
                } else {
                    let s = ds.current_scroll();
                    ds.set_current_scroll(s.saturating_add(1));
                }
                Some(Action::Render)
            }
            KeyCode::Enter => {
                if ds.active_tab == DetailTab::Conversation {
                    let key = format!("{}{}", detail::SECTION_MSG_PREFIX, ds.conv_cursor);
                    ds.toggle_section(key);
                }
                Some(Action::Render)
            }
            _ => None,
        }
    }
}

impl Component for Home {
    fn register_action_handler(&mut self, tx: UnboundedSender<Action>) -> color_eyre::Result<()> {
        self.command_tx = Some(tx);
        Ok(())
    }

    fn register_config_handler(&mut self, config: Config) -> color_eyre::Result<()> {
        self.sources = Arc::new(
            config
                .config
                .sources
                .iter()
                .filter(|c| c.enabled)
                .map(|c| source::create_source(c))
                .collect(),
        );
        self.config = config;
        Ok(())
    }

    fn init(&mut self, _area: Size) -> color_eyre::Result<()> {
        self.start_loading();
        Ok(())
    }

    fn update(&mut self, action: Action) -> color_eyre::Result<Option<Action>> {
        match action {
            Action::Tick => {
                if self.loading {
                    self.spinner_tick = self.spinner_tick.wrapping_add(1);
                    return Ok(Some(Action::Render));
                }
            }
            Action::TranscriptsLoaded => {
                if let Some(rx) = self.load_rx.take()
                    && let Ok(data) = rx.try_recv()
                {
                    self.finish_loading(data);
                    return Ok(Some(Action::Render));
                }
            }
            Action::RefreshTranscripts => {
                if !self.loading {
                    self.transcripts.clear();
                    self.display_names.clear();
                    self.start_loading();
                }
            }
            _ => {}
        }
        Ok(None)
    }

    fn handle_key_event(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> color_eyre::Result<Option<Action>> {
        let action = match &self.view {
            View::List => self.handle_list_key(key),
            View::Detail(_) => self.handle_detail_key(key),
        };
        Ok(action)
    }

    fn draw(&mut self, frame: &mut Frame, area: Rect) -> color_eyre::Result<()> {
        let [content_area, footer_area] =
            Layout::vertical([Constraint::Min(3), Constraint::Length(1)]).areas(area);

        if self.loading {
            let ch = SPINNER_CHARS[self.spinner_tick % SPINNER_CHARS.len()];
            let msg = Line::from(vec![
                Span::styled(format!("{ch} "), theme::braille_style()),
                Span::styled("Loading...", theme::body_style()),
            ]);
            let para = Paragraph::new(msg).alignment(Alignment::Center);
            frame.render_widget(
                para,
                Rect {
                    y: content_area.y + content_area.height / 2,
                    height: 1,
                    ..content_area
                },
            );
            let hint =
                Paragraph::new(footer::footer_hints(&[("q", "quit")])).alignment(Alignment::Center);
            frame.render_widget(hint, footer_area);
            return Ok(());
        }

        if self.transcripts.is_empty() {
            let default_dir = crate::config::get_default_transcript_dir();
            let msg = format!(
                "No transcripts found. Set transcript_dir in config or ensure {} exists.",
                default_dir.display()
            );
            let para = Paragraph::new(msg).style(theme::empty_msg_style());
            frame.render_widget(para, content_area);
            let hint = Paragraph::new(footer::footer_hints(&[("R", "refresh"), ("q", "quit")]))
                .alignment(Alignment::Center);
            frame.render_widget(hint, footer_area);
            return Ok(());
        }

        match &mut self.view {
            View::List => {
                let selected = self.table_state.selected();
                let rows: Vec<_> = self
                    .transcripts
                    .iter()
                    .enumerate()
                    .map(|(i, (_path, data))| {
                        let name = &self.display_names[i];
                        table_row(i, name, data, &self.expand_state, selected == Some(i))
                    })
                    .collect();
                let table = Table::new(rows, COLUMN_WIDTHS)
                    .header(table_header(&self.sort))
                    .column_spacing(2)
                    .style(theme::body_style())
                    .row_highlight_style(theme::highlight_style());
                frame.render_stateful_widget(table, content_area, &mut self.table_state);
                let can_expand = selected
                    .and_then(|i| self.transcripts.get(i))
                    .is_some_and(|(_, d)| d.models.len() > 1);
                let mut hints: Vec<(&str, &str)> = vec![("\u{2191}/\u{2193}", "navigate")];
                if can_expand {
                    hints.push(("\u{2192}/\u{2190}", "expand"));
                }
                hints.extend([
                    ("</>", "sort"),
                    ("s", "reverse"),
                    ("Enter", "detail"),
                    ("R", "refresh"),
                    ("q", "quit"),
                ]);
                let hint =
                    Paragraph::new(footer::footer_hints(&hints)).alignment(Alignment::Center);
                frame.render_widget(hint, footer_area);
            }
            View::Detail(ds) => {
                if ds.index < self.transcripts.len() {
                    let [header_area, tab_content_area] =
                        Layout::vertical([Constraint::Length(2), Constraint::Min(3)])
                            .areas(content_area);

                    let (path, data) = &self.transcripts[ds.index];

                    detail::render_detail_header(frame, header_area, path, data, ds.active_tab);

                    let content = match ds.active_tab {
                        DetailTab::Stats => {
                            detail::detail_stats_content(data, tab_content_area.width)
                        }
                        DetailTab::Conversation => {
                            let (content, cursor_line) = detail::detail_conversation_content(
                                self.detail_conversation.as_deref(),
                                &ds.expanded_sections,
                                ds.conv_cursor,
                                tab_content_area.width,
                                tab_content_area.height,
                            );
                            let visible = tab_content_area.height as usize;
                            let scroll = ds.current_scroll() as usize;
                            if cursor_line < scroll {
                                ds.set_current_scroll(cursor_line as u16);
                            } else if cursor_line >= scroll + visible {
                                ds.set_current_scroll(
                                    cursor_line.saturating_sub(visible / 3) as u16
                                );
                            }
                            content
                        }
                        DetailTab::Files => {
                            detail::detail_files_content(data, tab_content_area.width)
                        }
                    };

                    let max_scroll = content
                        .len()
                        .saturating_sub(tab_content_area.height as usize)
                        as u16;
                    let clamped = ds.current_scroll().min(max_scroll);
                    ds.set_current_scroll(clamped);

                    detail::render_scrollable_content(
                        frame,
                        tab_content_area,
                        content,
                        ds.current_scroll(),
                    );
                }
                let detail_hints: Vec<(&str, &str)> = match ds.active_tab {
                    DetailTab::Conversation => vec![
                        ("S/C/F", "tabs"),
                        ("j/k", "navigate"),
                        ("Enter", "expand"),
                        ("Esc", "back"),
                    ],
                    _ => vec![("S/C/F", "tabs"), ("j/k", "scroll"), ("Esc", "back")],
                };
                let hint = Paragraph::new(footer::footer_hints(&detail_hints))
                    .alignment(Alignment::Center);
                frame.render_widget(hint, footer_area);
            }
        }
        Ok(())
    }
}

// ── Background loading ──────────────────────────────────────────────────────

fn run_load(sources: &[Box<dyn TranscriptSource>], config: &Config) -> SessionList {
    let t_total = std::time::Instant::now();

    let jobs: Vec<_> = sources
        .iter()
        .filter_map(|src| {
            let cfg = config
                .config
                .sources
                .iter()
                .find(|c| c.kind == src.kind() && c.enabled)?;
            Some((src.as_ref(), cfg))
        })
        .collect();

    let results: Vec<SessionList> = std::thread::scope(|s| {
        let handles: Vec<_> = jobs
            .iter()
            .map(|&(src, cfg)| {
                s.spawn(move || {
                    let t0 = std::time::Instant::now();
                    let paths = src.discover_paths(&cfg.root_dir);
                    tracing::info!(
                        "[home] {:?} discover_paths ({} paths): {:?}",
                        src.kind(),
                        paths.len(),
                        t0.elapsed()
                    );

                    let t0 = std::time::Instant::now();
                    let loaded = src.load_transcripts(&paths);
                    tracing::info!(
                        "[home] {:?} load_transcripts ({} results): {:?}",
                        src.kind(),
                        loaded.len(),
                        t0.elapsed()
                    );
                    loaded
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_default())
            .collect()
    });

    let mut all: SessionList = Vec::new();
    for batch in results {
        all.extend(batch);
    }
    for (_, data) in &mut all {
        data.compute_cost();
    }

    tracing::info!(
        "[home] load_transcripts total ({} sessions): {:?}",
        all.len(),
        t_total.elapsed()
    );
    all
}
