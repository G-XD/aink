//! Source-agnostic transcript data models.
//!
//! These types are shared across all transcript sources (Claude, Codex, Cursor, …).
//! Source-specific parsing lives in sibling modules (e.g. `claude.rs`).

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use super::source::SourceKind;

/// Per-model token usage breakdown.
#[derive(Clone, Debug, Default)]
pub struct ModelStats {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub tool_call_count: u64,
}

/// Aggregated data from a transcript session.
#[derive(Clone)]
#[allow(dead_code)]
pub struct TranscriptData {
    pub source: SourceKind,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub models: HashSet<String>,
    pub tool_call_total: u64,
    pub tool_call_by_type: HashMap<String, u64>,
    pub files_touched: Vec<String>,
    pub first_user_message: Option<String>,
    pub per_model: HashMap<String, ModelStats>,
    pub duration_ms: u64,
    pub turn_count: u64,
    pub user_message_count: u64,
    pub assistant_message_count: u64,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub agent_version: Option<String>,
    pub git_branch: Option<String>,
    pub slug: Option<String>,
    /// Human-readable project name, set by each source at parse time.
    pub project_name: Option<String>,
    pub estimated_cost_usd: f64,
}

/// Structured detail for a single tool call within a conversation turn.
#[derive(Clone, Debug)]
pub struct ToolCallDetail {
    pub name: String,
    /// One-line summary of the most relevant input parameter (e.g. file path, command).
    pub summary: String,
}

/// One turn in the conversation (user or assistant message).
#[derive(Clone, Debug)]
pub struct ConversationTurn {
    pub role: ConversationRole,
    pub content: String,
    pub tool_calls: Vec<ToolCallDetail>,
    /// ISO 8601 timestamp when the message was sent (e.g. from `createdAt` / `timestamp`).
    pub created_at: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConversationRole {
    User,
    Assistant,
}

impl TranscriptData {
    /// Recompute `estimated_cost_usd` from `per_model` using the global price
    /// table.  Call this once after all mutations (parsing, subagent merging,
    /// backfill) are complete.
    ///
    /// When `per_model` entries exist but carry no tokens (common for Cursor
    /// where DB token counts are often zero), the session-level totals are
    /// distributed across known models so cost estimation still works.
    pub fn compute_cost(&mut self) {
        self.backfill_per_model_tokens();
        self.estimated_cost_usd = self
            .per_model
            .iter()
            .map(|(model, stats)| crate::utils::prices::estimate_cost(model, stats))
            .sum();
    }

    /// If `per_model` has entries with zero tokens but session-level totals
    /// are non-zero, distribute the session totals evenly.
    fn backfill_per_model_tokens(&mut self) {
        if self.per_model.is_empty() || (self.input_tokens == 0 && self.output_tokens == 0) {
            return;
        }
        let tracked: u64 = self
            .per_model
            .values()
            .map(|s| s.input_tokens + s.output_tokens)
            .sum();
        if tracked > 0 {
            return;
        }
        let n = self.per_model.len() as u64;
        let (share_in, share_out) = (self.input_tokens / n, self.output_tokens / n);
        let (mut rem_in, mut rem_out) = (self.input_tokens % n, self.output_tokens % n);
        for ms in self.per_model.values_mut() {
            ms.input_tokens = share_in
                + if rem_in > 0 {
                    rem_in -= 1;
                    1
                } else {
                    0
                };
            ms.output_tokens = share_out
                + if rem_out > 0 {
                    rem_out -= 1;
                    1
                } else {
                    0
                };
        }
    }
}

/// Type alias for the common transcript list used across components.
pub type SessionList = Vec<(PathBuf, TranscriptData)>;
