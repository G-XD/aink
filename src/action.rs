use serde::{Deserialize, Serialize};
use strum::Display;

#[derive(Debug, Clone, PartialEq, Eq, Display, Serialize, Deserialize)]
pub enum Action {
    Tick,
    Render,
    Resize(u16, u16),
    Suspend,
    Resume,
    Quit,
    ClearScreen,
    Error(String),
    Help,
    RefreshTranscripts,
    TranscriptsLoaded,
    TabNext,
    TabPrev,
    TabSelect(usize),
    ExpandRow,
    CollapseRow,
    /// Trigger session export with session index
    ExportSession(usize),
    /// Export completion feedback
    /// Ok contains file path, Err contains error message
    ExportComplete(Result<String, String>),
}
