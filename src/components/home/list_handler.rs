//! Key event handlers for the list view.

use crossterm::event::{KeyCode, KeyEvent};

use super::{
    Home,
    table::{selection_next, selection_previous, set_selection},
};
use crate::action::Action;

impl Home {
    /// Handle key events when in list view.
    pub(super) fn handle_list_key(&mut self, key: KeyEvent) -> Option<Action> {
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
                    self.apply_filter_and_sort();
                    self.expand_state = Default::default();
                    set_selection(&mut self.table_state, Some(0));
                    Some(Action::Render)
                } else {
                    None
                }
            }
            KeyCode::Char('<') | KeyCode::Char(',') => {
                if let Some(prev) = self.sort.column.prev() {
                    self.sort.column = prev;
                    self.apply_filter_and_sort();
                    self.expand_state = Default::default();
                    set_selection(&mut self.table_state, Some(0));
                    Some(Action::Render)
                } else {
                    None
                }
            }
            KeyCode::Char('s') => {
                self.sort.ascending = !self.sort.ascending;
                self.apply_filter_and_sort();
                self.expand_state = Default::default();
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
            KeyCode::Char('e') => {
                if let Some(i) = self.table_state.selected() {
                    return Some(Action::ExportSession(i));
                }
                None
            }
            _ => None,
        }
    }

    fn table_next(&mut self) {
        let i = selection_next(self.table_state.selected(), self.transcripts.len());
        set_selection(&mut self.table_state, i);
    }

    fn table_previous(&mut self) {
        let i = selection_previous(self.table_state.selected(), self.transcripts.len());
        set_selection(&mut self.table_state, i);
    }
}
