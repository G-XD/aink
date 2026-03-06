//! View state: list (with per-row expand) vs detail (with sub-tabs).

use std::collections::HashSet;

/// Sub-tab within the detail view.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum DetailTab {
    #[default]
    Stats,
    Conversation,
    Files,
}

pub const DETAIL_TAB_COUNT: usize = 3;

impl DetailTab {
    pub fn index(self) -> usize {
        match self {
            Self::Stats => 0,
            Self::Conversation => 1,
            Self::Files => 2,
        }
    }

    pub fn next(self) -> Self {
        match self {
            Self::Stats => Self::Conversation,
            Self::Conversation => Self::Files,
            Self::Files => Self::Stats,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Self::Stats => Self::Files,
            Self::Conversation => Self::Stats,
            Self::Files => Self::Conversation,
        }
    }
}

pub const DETAIL_TAB_LABELS: &[&str] = &["Stats", "Conversation", "Files"];

/// State for the detail (single-session) view, kept as a standalone struct
/// so mutations don't require reconstructing the entire `View` enum.
#[derive(Default)]
pub struct DetailState {
    pub index: usize,
    pub active_tab: DetailTab,
    /// Independent scroll position per sub-tab: [Stats, Conversation, Files].
    pub tab_scrolls: [u16; DETAIL_TAB_COUNT],
    /// Which foldable sections are expanded (by section id).
    pub expanded_sections: HashSet<String>,
    /// Cursor position within the Conversation tab (turn index).
    pub conv_cursor: usize,
    /// Previous cursor value — used by draw to detect cursor changes
    /// and only auto-scroll when cursor actually moved to a new message.
    prev_conv_cursor: Option<usize>,
}

impl DetailState {
    pub fn new(index: usize) -> Self {
        Self {
            index,
            ..Default::default()
        }
    }

    pub fn current_scroll(&self) -> u16 {
        self.tab_scrolls[self.active_tab.index()]
    }

    pub fn set_current_scroll(&mut self, val: u16) {
        self.tab_scrolls[self.active_tab.index()] = val;
    }

    /// Returns true if conv_cursor changed since the last call, then updates tracking.
    pub fn take_cursor_changed(&mut self) -> bool {
        let changed = self.prev_conv_cursor != Some(self.conv_cursor);
        self.prev_conv_cursor = Some(self.conv_cursor);
        changed
    }

    pub fn toggle_section(&mut self, key: String) {
        if !self.expanded_sections.remove(&key) {
            self.expanded_sections.insert(key);
        }
    }
}

#[derive(Default)]
pub enum View {
    #[default]
    List,
    Detail(DetailState),
}

/// Sortable column in the sessions table.
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum SortColumn {
    Input,
    Output,
    Total,
    #[default]
    Active,
    Cost,
}

impl SortColumn {
    pub fn next(self) -> Option<Self> {
        match self {
            Self::Input => Some(Self::Output),
            Self::Output => Some(Self::Total),
            Self::Total => Some(Self::Active),
            Self::Active => Some(Self::Cost),
            Self::Cost => None,
        }
    }

    pub fn prev(self) -> Option<Self> {
        match self {
            Self::Input => None,
            Self::Output => Some(Self::Input),
            Self::Total => Some(Self::Output),
            Self::Active => Some(Self::Total),
            Self::Cost => Some(Self::Active),
        }
    }
}

/// Current sort column and direction.
#[derive(Clone, Copy)]
pub struct SortState {
    pub column: SortColumn,
    pub ascending: bool,
}

impl Default for SortState {
    fn default() -> Self {
        Self {
            column: SortColumn::Active,
            ascending: false,
        }
    }
}

/// Tracks which session rows are expanded to show per-model breakdown.
#[derive(Default)]
pub struct ExpandState {
    pub expanded: HashSet<usize>,
}

impl ExpandState {
    pub fn toggle(&mut self, index: usize) {
        if !self.expanded.remove(&index) {
            self.expanded.insert(index);
        }
    }

    pub fn is_expanded(&self, index: usize) -> bool {
        self.expanded.contains(&index)
    }
}
