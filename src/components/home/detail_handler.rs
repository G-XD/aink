//! Key event handlers for the detail view.

use crossterm::event::{KeyCode, KeyEvent};

use super::{
    Home, detail,
    view::{DetailTab, View},
};
use crate::action::Action;

impl Home {
    /// Handle key events when in detail view.
    pub(super) fn handle_detail_key(&mut self, key: KeyEvent) -> Option<Action> {
        // Ensure we're in detail view
        let is_detail = matches!(self.view, View::Detail(_));
        if !is_detail {
            return None;
        }

        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => {
                self.go_back_to_list();
                Some(Action::Render)
            }
            KeyCode::Char('S') => {
                if let View::Detail(ds) = &mut self.view {
                    ds.active_tab = DetailTab::Stats;
                }
                Some(Action::Render)
            }
            KeyCode::Char('C') => {
                if let View::Detail(ds) = &mut self.view {
                    ds.active_tab = DetailTab::Conversation;
                }
                Some(Action::Render)
            }
            KeyCode::Char('F') => {
                if let View::Detail(ds) = &mut self.view {
                    ds.active_tab = DetailTab::Files;
                }
                Some(Action::Render)
            }
            KeyCode::Tab => {
                if let View::Detail(ds) = &mut self.view {
                    ds.active_tab = ds.active_tab.next();
                }
                Some(Action::Render)
            }
            KeyCode::BackTab => {
                if let View::Detail(ds) = &mut self.view {
                    ds.active_tab = ds.active_tab.prev();
                }
                Some(Action::Render)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.handle_detail_up();
                Some(Action::Render)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.handle_detail_down();
                Some(Action::Render)
            }
            KeyCode::Enter => {
                self.handle_detail_enter();
                Some(Action::Render)
            }
            KeyCode::Char('e') => {
                if let View::Detail(ds) = &self.view {
                    Some(Action::ExportSession(ds.index))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Handle upward navigation in detail view.
    fn handle_detail_up(&mut self) {
        let View::Detail(ds) = &mut self.view else {
            return;
        };

        let conv_len = self.detail_conversation.as_ref().map_or(0, |c| c.len());

        if ds.active_tab == DetailTab::Conversation && conv_len > 0 {
            self.handle_conversation_up(conv_len);
        } else {
            let s = ds.current_scroll();
            ds.set_current_scroll(s.saturating_sub(1));
        }
    }

    /// Handle upward navigation within conversation view.
    fn handle_conversation_up(&mut self, _conv_len: usize) {
        let View::Detail(ds) = &mut self.view else {
            return;
        };

        if ds.conv_cursor == 0 {
            // Already at first message, just scroll up
            let s = ds.current_scroll();
            ds.set_current_scroll(s.saturating_sub(1));
        } else if let Some(conv) = self.detail_conversation.as_deref() {
            let viewport_height = self.last_render_height.unwrap_or(20);
            let width = self.last_render_width.unwrap_or(80);
            let visible = viewport_height as usize;
            let (_, cursor_line, _, _) = detail::detail_conversation_content(
                Some(conv),
                &ds.expanded_sections,
                ds.conv_cursor,
                width,
                viewport_height,
            );

            let scroll = ds.current_scroll() as usize;

            if scroll <= cursor_line {
                // At top of current message — jump to previous message.
                ds.conv_cursor = ds.conv_cursor.saturating_sub(1);

                let (_, prev_cursor_line, _, _) = detail::detail_conversation_content(
                    Some(conv),
                    &ds.expanded_sections,
                    ds.conv_cursor,
                    width,
                    viewport_height,
                );

                if prev_cursor_line < scroll {
                    // Previous message is above viewport — show its
                    // bottom so user can scroll upward through it.
                    let bottom_scroll = cursor_line.saturating_sub(visible);
                    ds.set_current_scroll(bottom_scroll.max(prev_cursor_line) as u16);
                }
                // Otherwise prev message is already visible; keep scroll.
            } else {
                // Still can scroll up within current message
                ds.set_current_scroll(scroll.saturating_sub(1) as u16);
            }
        } else {
            ds.conv_cursor = ds.conv_cursor.saturating_sub(1);
        }
    }

    /// Handle downward navigation in detail view.
    fn handle_detail_down(&mut self) {
        let View::Detail(ds) = &mut self.view else {
            return;
        };

        let conv_len = self.detail_conversation.as_ref().map_or(0, |c| c.len());

        if ds.active_tab == DetailTab::Conversation && conv_len > 0 {
            self.handle_conversation_down(conv_len);
        } else {
            let s = ds.current_scroll();
            ds.set_current_scroll(s.saturating_add(1));
        }
    }

    /// Handle downward navigation within conversation view.
    fn handle_conversation_down(&mut self, conv_len: usize) {
        let View::Detail(ds) = &mut self.view else {
            return;
        };

        if ds.conv_cursor >= conv_len.saturating_sub(1) {
            // Already at last message, just scroll down
            let s = ds.current_scroll();
            ds.set_current_scroll(s.saturating_add(1));
        } else if let Some(conv) = self.detail_conversation.as_deref() {
            let viewport_height = self.last_render_height.unwrap_or(20);
            let visible = viewport_height as usize;
            let scroll = ds.current_scroll() as usize;

            // Get current view's total_lines to calculate max_scroll
            let (_, _current_start, total_lines, _) = detail::detail_conversation_content(
                Some(conv),
                &ds.expanded_sections,
                ds.conv_cursor,
                self.last_render_width.unwrap_or(80),
                viewport_height,
            );

            // Calculate max_scroll - same as render logic
            let max_scroll = total_lines.saturating_sub(visible);

            // Get next message start position
            let (_, next_start, _, _) = detail::detail_conversation_content(
                Some(conv),
                &ds.expanded_sections,
                ds.conv_cursor + 1,
                self.last_render_width.unwrap_or(80),
                viewport_height,
            );

            // Jump to next message if:
            // 1. Next message is visible in viewport, OR
            // 2. We've reached max_scroll (can't scroll further)
            if next_start < scroll + visible || scroll >= max_scroll {
                ds.conv_cursor = (ds.conv_cursor + 1).min(conv_len - 1);
                // If next message is below viewport, scroll to it
                if next_start >= scroll + visible {
                    ds.set_current_scroll(next_start as u16);
                }
            } else {
                // Can still scroll down
                ds.set_current_scroll((scroll + 1) as u16);
            }
        } else {
            // Fallback: just move cursor
            ds.conv_cursor = (ds.conv_cursor + 1).min(conv_len - 1);
        }
    }

    /// Handle Enter key in detail view (toggle message expansion).
    fn handle_detail_enter(&mut self) {
        let View::Detail(ds) = &mut self.view else {
            return;
        };

        if ds.active_tab == DetailTab::Conversation {
            let key = format!("{}{}", detail::SECTION_MSG_PREFIX, ds.conv_cursor);
            ds.toggle_section(key);
            // Snap scroll to cursor after toggle so the viewport
            // doesn't jump when a tall expanded message collapses.
            if let Some(conv) = self.detail_conversation.as_deref() {
                let (_, cursor_line, _, _) = detail::detail_conversation_content(
                    Some(conv),
                    &ds.expanded_sections,
                    ds.conv_cursor,
                    self.last_render_width.unwrap_or(80),
                    self.last_render_height.unwrap_or(20),
                );
                ds.set_current_scroll(cursor_line as u16);
            }
        }
    }
}
