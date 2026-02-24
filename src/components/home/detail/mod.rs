//! Session detail view with sub-tabs: Stats, Conversation, Files.

mod conversation;
mod files;
mod header;
mod stats;

pub use conversation::{SECTION_MSG_PREFIX, detail_conversation_content};
pub use files::detail_files_content;
pub use header::render_detail_header;
pub use stats::detail_stats_content;

use ratatui::layout::Rect;
use ratatui::prelude::*;
use ratatui::widgets::{Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState};

/// Render a list of lines as a scrollable Paragraph with a scrollbar.
///
/// Uses line-skip scrolling (no word-wrap) so cost is O(1) per skipped line
/// regardless of scroll depth. Content builders handle width themselves.
pub fn render_scrollable_content(
    frame: &mut Frame,
    area: Rect,
    content: Vec<Line<'static>>,
    scroll: u16,
) {
    let content_len = content.len();
    let para = Paragraph::new(content).scroll((scroll, 0));
    frame.render_widget(para, area);

    let mut scrollbar_state = ScrollbarState::new(content_len).position(scroll as usize);
    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None),
        area,
        &mut scrollbar_state,
    );
}
