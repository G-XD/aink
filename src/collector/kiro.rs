//! Kiro AI assistant transcript parser.
//!
//! Kiro stores execution data under its globalStorage directory:
//!
//! ```text
//! kiro.kiroagent/
//! ├── <profile-hash>/
//! │   ├── <exec-index-hash>       (JSON: { executions: [...] })
//! │   ├── <exec-data-dir>/        (execution data files)
//! │   │   └── <hash>.json         (full context, actions, usageSummary)
//! │   ├── <file-snapshots-dir>/   (tool output file snapshots)
//! │   └── <task-metadata-dir>/    (task tracking)
//! └── workspace-sessions/         (lightweight UI chat history)
//! ```
//!
//! Multiple execution data files share the same `chatSessionId`, representing
//! separate agent runs within one chat tab. We group and merge them by
//! `chatSessionId` so each Kiro chat tab maps to one session in aink.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use tracing::warn;

use super::source::{SourceKind, TranscriptSource};
use super::transcript::{ConversationRole, ConversationTurn, ToolCallDetail, TranscriptData};

// ── Kiro execution data JSON schema ─────────────────────────────────────────

/// Full execution data file.
#[derive(Debug, Deserialize)]
struct KiroExecData {
    #[serde(rename = "executionId", default)]
    _execution_id: Option<String>,
    #[serde(rename = "workflowType", default)]
    workflow_type: Option<String>,
    #[serde(default)]
    _status: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(rename = "startTime", default)]
    start_time: Option<i64>,
    #[serde(rename = "endTime", default)]
    end_time: Option<i64>,
    #[serde(default)]
    actions: Vec<serde_json::Value>,
    #[serde(default)]
    context: Option<KiroContext>,
    #[serde(rename = "usageSummary", default)]
    usage_summary: Option<Vec<KiroUsageSummary>>,
    #[serde(rename = "chatSessionId", default)]
    chat_session_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct KiroContext {
    #[serde(default)]
    messages: Vec<KiroContextMessage>,
}

#[derive(Debug, Deserialize)]
struct KiroContextMessage {
    #[serde(default)]
    role: String,
    #[serde(default)]
    entries: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct KiroUsageSummary {
    #[serde(default)]
    usage: Option<f64>,
    #[serde(default)]
    _unit: Option<String>,
    #[serde(rename = "usedTools", default)]
    _used_tools: Option<Vec<String>>,
}

/// Session index entry in `workspace-sessions/<base64-dir>/sessions.json`.
#[derive(Debug, Deserialize)]
struct KiroSessionIndex {
    #[serde(rename = "sessionId")]
    session_id: String,
    #[serde(default)]
    _title: Option<String>,
    #[serde(rename = "workspaceDirectory", default)]
    workspace_directory: Option<String>,
}

// ── Source implementation ────────────────────────────────────────────────────

pub struct KiroSource;

impl TranscriptSource for KiroSource {
    fn kind(&self) -> SourceKind {
        SourceKind::Kiro
    }

    fn discover_paths(&self, root: &Path) -> Vec<PathBuf> {
        if !root.is_dir() {
            return Vec::new();
        }

        let mut all_exec_files = Vec::new();

        // Walk profile-hash directories at root level
        let read_dir = match std::fs::read_dir(root) {
            Ok(rd) => rd,
            Err(e) => {
                warn!("read_dir {}: {}", root.display(), e);
                return Vec::new();
            }
        };

        for entry in read_dir.flatten() {
            let profile_dir = entry.path();
            if !profile_dir.is_dir() {
                continue;
            }
            let name = profile_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            if matches!(
                name,
                "workspace-sessions"
                    | "default"
                    | "index"
                    | "dev_data"
                    | ".diffs"
                    | ".migrations"
                    | ".utils"
            ) {
                continue;
            }

            find_execution_data_files(&profile_dir, &mut all_exec_files);
        }

        // Group execution files by chatSessionId and return one representative
        // path per group (the newest file). Files without chatSessionId each
        // become their own group.
        let mut groups: HashMap<String, Vec<PathBuf>> = HashMap::new();
        let mut ungrouped = Vec::new();

        for path in &all_exec_files {
            if let Some(sid) = read_chat_session_id(path) {
                groups.entry(sid).or_default().push(path.clone());
            } else {
                ungrouped.push(path.clone());
            }
        }

        let mut paths: Vec<PathBuf> = Vec::new();

        // For each chatSessionId group, pick the newest file as representative
        for (_sid, mut files) in groups {
            sort_by_mtime_desc(&mut files);
            if let Some(newest) = files.into_iter().next() {
                paths.push(newest);
            }
        }

        // Ungrouped files are individual sessions
        paths.extend(ungrouped);

        // Sort all by modification time (newest first)
        sort_by_mtime_desc(&mut paths);

        paths
    }

    fn parse_transcript(&self, path: &Path) -> color_eyre::Result<TranscriptData> {
        use color_eyre::eyre::WrapErr;

        // Load the primary execution file
        let content = std::fs::read_to_string(path)
            .wrap_err_with(|| format!("读取文件失败: {}", path.display()))?;
        let primary: KiroExecData = serde_json::from_str(&content)
            .wrap_err_with(|| format!("解析 JSON 失败: {}", path.display()))?;

        // Collect all executions for the same chatSessionId
        let all_execs = collect_session_executions(path, &primary);

        let mut tool_counts: HashMap<String, u64> = HashMap::new();
        let mut tool_total = 0u64;
        let mut files_touched = HashSet::new();
        let mut user_message_count = 0u64;
        let mut assistant_message_count = 0u64;
        let mut first_user_message: Option<String> = None;
        let mut models = HashSet::new();
        let mut total_credits = 0.0f64;
        let mut earliest_start: Option<i64> = None;
        let mut latest_end: Option<i64> = None;
        let mut title: Option<String> = None;

        for exec in &all_execs {
            // Accumulate actions/tools
            for action in &exec.actions {
                let action_type = action
                    .get("actionType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                if action_type == "model" || action_type.is_empty() {
                    continue;
                }

                let tool_type = identify_tool_type(action_type);

                if matches!(
                    tool_type.as_str(),
                    "file_write" | "file_edit" | "file_delete"
                ) && let Some(input) = action.get("input")
                {
                    for key in &["file", "path", "filePath"] {
                        if let Some(p) = input.get(*key).and_then(|v| v.as_str()) {
                            files_touched.insert(p.to_string());
                        }
                    }
                }

                *tool_counts.entry(tool_type).or_insert(0) += 1;
                tool_total += 1;
            }

            // Accumulate messages
            if let Some(ref ctx) = exec.context {
                for msg in &ctx.messages {
                    match msg.role.as_str() {
                        "human" | "user" => {
                            user_message_count += 1;
                            if first_user_message.is_none() {
                                first_user_message = extract_text_from_entries(&msg.entries)
                                    .map(|t| truncate_message(&t, 200));
                            }
                        }
                        "bot" | "assistant" => {
                            assistant_message_count += 1;
                        }
                        _ => {}
                    }
                }
            }

            // Accumulate credits
            if let Some(ref usage) = exec.usage_summary {
                for u in usage {
                    total_credits += u.usage.unwrap_or(0.0);
                }
            }

            // Track model/workflow types
            if let Some(ref wt) = exec.workflow_type {
                models.insert(wt.clone());
            }

            // Track time range across all executions
            if let Some(s) = exec.start_time {
                earliest_start = Some(earliest_start.map_or(s, |prev: i64| prev.min(s)));
            }
            if let Some(e) = exec.end_time {
                latest_end = Some(latest_end.map_or(e, |prev: i64| prev.max(e)));
            }

            // Use the first non-empty title
            if title.is_none() {
                title = exec.title.clone().or_else(|| exec.workflow_type.clone());
            }
        }

        if models.is_empty() {
            models.insert("kiro-agent".to_string());
        }

        let duration_ms = match (earliest_start, latest_end) {
            (Some(s), Some(e)) => ((e - s).max(0)) as u64,
            _ => 0,
        };

        let start_time = earliest_start.map(ms_to_iso8601);
        let end_time = latest_end.map(ms_to_iso8601);

        let project_name = find_project_name(path, primary.chat_session_id.as_deref());

        let mut files_vec: Vec<String> = files_touched.into_iter().collect();
        files_vec.sort();

        Ok(TranscriptData {
            source: SourceKind::Kiro,
            input_tokens: 0,
            output_tokens: 0,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            models,
            tool_call_total: tool_total,
            tool_call_by_type: tool_counts,
            files_touched: files_vec,
            first_user_message,
            summary: title,
            per_model: HashMap::new(),
            duration_ms,
            turn_count: user_message_count,
            user_message_count,
            assistant_message_count,
            start_time,
            end_time,
            agent_version: None,
            git_branch: None,
            slug: primary.workflow_type,
            project_name,
            estimated_cost_usd: total_credits,
        })
    }

    fn parse_conversation(&self, path: &Path) -> color_eyre::Result<Vec<ConversationTurn>> {
        use color_eyre::eyre::WrapErr;

        let content = std::fs::read_to_string(path)
            .wrap_err_with(|| format!("读取文件失败: {}", path.display()))?;
        let primary: KiroExecData = serde_json::from_str(&content)
            .wrap_err_with(|| format!("解析 JSON 失败: {}", path.display()))?;

        let all_execs = collect_session_executions(path, &primary);

        let mut turns = Vec::new();

        for exec in &all_execs {
            // Build tool call details from actions for this execution
            let action_tools: Vec<ToolCallDetail> = exec
                .actions
                .iter()
                .filter_map(|a| {
                    let action_type = a.get("actionType")?.as_str()?;
                    if action_type == "model" {
                        return None;
                    }
                    let summary = a
                        .get("input")
                        .map(extract_action_summary)
                        .unwrap_or_default();
                    Some(ToolCallDetail {
                        name: action_type.to_string(),
                        summary,
                    })
                })
                .collect();

            let mut exec_turns = Vec::new();

            if let Some(ref ctx) = exec.context {
                for msg in &ctx.messages {
                    let role = match msg.role.as_str() {
                        "human" | "user" => ConversationRole::User,
                        "bot" | "assistant" => ConversationRole::Assistant,
                        _ => continue,
                    };

                    let mut text_parts = Vec::new();
                    let mut tool_calls = Vec::new();

                    for entry in &msg.entries {
                        let entry_type = entry.get("type").and_then(|v| v.as_str()).unwrap_or("");

                        match entry_type {
                            "text" => {
                                if let Some(t) = entry.get("text").and_then(|v| v.as_str())
                                    && !t.starts_with("# System Prompt")
                                    && !t.starts_with("<identity>")
                                    && !t.trim().is_empty()
                                {
                                    text_parts.push(t.to_string());
                                }
                            }
                            "toolUse" => {
                                let name = entry
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown")
                                    .to_string();
                                let summary = entry
                                    .get("args")
                                    .map(extract_action_summary)
                                    .unwrap_or_default();
                                tool_calls.push(ToolCallDetail { name, summary });
                            }
                            _ => {}
                        }
                    }

                    let content_text = text_parts.join("\n");
                    if content_text.trim().is_empty() && tool_calls.is_empty() {
                        continue;
                    }

                    exec_turns.push(ConversationTurn {
                        role,
                        content: content_text,
                        tool_calls,
                        created_at: None,
                    });
                }
            }

            // Attach action tools to last assistant turn if no inline tools
            if exec_turns.iter().all(|t| t.tool_calls.is_empty())
                && !action_tools.is_empty()
                && let Some(last) = exec_turns
                    .iter_mut()
                    .rev()
                    .find(|t| t.role == ConversationRole::Assistant)
            {
                last.tool_calls = action_tools;
            }

            turns.extend(exec_turns);
        }

        Ok(turns)
    }

    fn load_transcripts(&self, paths: &[PathBuf]) -> Vec<(PathBuf, TranscriptData)> {
        let mut out = Vec::with_capacity(paths.len());

        for path in paths {
            match self.parse_transcript(path) {
                Ok(data) => {
                    // Include if there are messages or tool calls (Kiro has no raw tokens)
                    if data.user_message_count > 0
                        || data.assistant_message_count > 0
                        || data.tool_call_total > 0
                    {
                        out.push((path.clone(), data));
                    }
                }
                Err(e) => {
                    warn!("解析会话失败 {}: {}", path.display(), e);
                }
            }
        }

        out
    }
}

// ── Helper functions ─────────────────────────────────────────────────────────

/// Read the chatSessionId from an execution data file without full parsing.
fn read_chat_session_id(path: &Path) -> Option<String> {
    let content = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;
    v.get("chatSessionId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Sort paths by modification time, newest first.
fn sort_by_mtime_desc(paths: &mut [PathBuf]) {
    paths.sort_by(|a, b| {
        let t_a = std::fs::metadata(a).and_then(|m| m.modified()).ok();
        let t_b = std::fs::metadata(b).and_then(|m| m.modified()).ok();
        match (t_a, t_b) {
            (Some(ta), Some(tb)) => tb.cmp(&ta),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
    });
}

/// Collect all execution data files that share the same chatSessionId as `primary`.
/// Returns them sorted by startTime ascending so conversations are in order.
fn collect_session_executions(path: &Path, primary: &KiroExecData) -> Vec<KiroExecData> {
    let session_id = match &primary.chat_session_id {
        Some(sid) => sid.clone(),
        None => return vec![parse_exec_file(path).unwrap_or_else(|| unreachable!())],
    };

    // Find sibling execution files in the same directory
    let parent = match path.parent() {
        Some(p) => p,
        None => return vec![parse_exec_file(path).unwrap_or_else(|| unreachable!())],
    };

    let mut execs: Vec<KiroExecData> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(parent) {
        for entry in rd.flatten() {
            let fp = entry.path();
            if !fp.is_file() {
                continue;
            }
            if let Some(exec) = parse_exec_file(&fp)
                && exec.chat_session_id.as_deref() == Some(session_id.as_str())
            {
                execs.push(exec);
            }
        }
    }

    if execs.is_empty() {
        // Fallback: just use the primary
        if let Some(exec) = parse_exec_file(path) {
            return vec![exec];
        }
        return Vec::new();
    }

    // Sort by startTime ascending
    execs.sort_by_key(|e| e.start_time.unwrap_or(i64::MAX));
    execs
}

/// Parse a single execution data file.
fn parse_exec_file(path: &Path) -> Option<KiroExecData> {
    let content = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Find execution data directories within a profile directory.
///
/// Inside a profile dir there are:
/// - Non-directory files (execution index, config)
/// - Directories with large JSON files (execution data) ← what we want
/// - Directories with source file snapshots (tool artifacts) ← skip
///
/// An execution data directory contains only `.json`-like files (no extension,
/// but parseable as JSON with `executionId`).
fn find_execution_data_files(profile_dir: &Path, out: &mut Vec<PathBuf>) {
    let rd = match std::fs::read_dir(profile_dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };

    for entry in rd.flatten() {
        let sub = entry.path();
        if !sub.is_dir() {
            continue;
        }

        // Check if this subdir contains execution data files.
        // Execution data files are large (>1KB), have no extension, and are JSON.
        // File snapshot dirs contain actual source files with extensions.
        let sub_rd = match std::fs::read_dir(&sub) {
            Ok(rd) => rd,
            Err(_) => continue,
        };

        let mut candidate_files = Vec::new();
        let mut has_source_dirs = false;

        for file_entry in sub_rd.flatten() {
            let fp = file_entry.path();
            if fp.is_dir() {
                // Source file snapshot directories contain dirs like "src/"
                has_source_dirs = true;
                break;
            }
            // Execution data files have no extension and are reasonably large
            if (fp.extension().is_none() || fp.extension().is_some_and(|e| e == "json"))
                && let Ok(meta) = std::fs::metadata(&fp)
                && meta.len() > 100
            {
                candidate_files.push(fp);
            }
        }

        if has_source_dirs || candidate_files.is_empty() {
            continue;
        }

        // Validate: try to parse the first file to confirm it's execution data
        if let Some(first) = candidate_files.first()
            && is_execution_data_file(first)
        {
            out.extend(candidate_files);
        }
    }
}

/// Quick check if a file looks like a Kiro execution data file.
fn is_execution_data_file(path: &Path) -> bool {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    // Must be JSON with executionId field
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
        v.get("executionId").is_some()
    } else {
        false
    }
}

/// Map Kiro action types to unified tool classifications.
fn identify_tool_type(action_type: &str) -> String {
    match action_type {
        "readFiles" | "readFile" | "readMultipleFiles" | "readCode" => "file_read".to_string(),
        "write" | "fsWrite" | "fsAppend" => "file_write".to_string(),
        "create" | "append" => "file_write".to_string(),
        "editCode" | "strReplace" | "semanticRename" | "smartRelocate" => "file_edit".to_string(),
        "search" | "grepSearch" | "fileSearch" => "search".to_string(),
        "getDiagnostics" | "preWork" => "diagnostics".to_string(),
        "userInput" | "say" => "user_interaction".to_string(),
        "invokeSubAgent" | "subagentResponse" => "subagent".to_string(),
        "listDirectory" => "directory".to_string(),
        "deleteFile" => "file_delete".to_string(),
        "executeBash" | "controlBashProcess" => "shell".to_string(),
        _ => action_type.to_string(),
    }
}

/// Extract text content from context message entries.
fn extract_text_from_entries(entries: &[serde_json::Value]) -> Option<String> {
    let mut parts = Vec::new();
    for entry in entries {
        if entry.get("type").and_then(|v| v.as_str()) == Some("text")
            && let Some(t) = entry.get("text").and_then(|v| v.as_str())
        {
            // Skip system prompts and identity blocks
            if !t.starts_with("# System Prompt")
                && !t.starts_with("<identity>")
                && !t.trim().is_empty()
            {
                parts.push(t.to_string());
            }
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

/// Truncate a message to `max_len` characters, appending "..." if truncated.
fn truncate_message(text: &str, max_len: usize) -> String {
    if text.len() <= max_len {
        return text.to_string();
    }
    let end = text
        .char_indices()
        .take(max_len)
        .last()
        .map(|(i, _)| i)
        .unwrap_or(text.len());
    format!("{}...", &text[..end])
}

/// Extract a one-line summary from an action's input parameters.
fn extract_action_summary(input: &serde_json::Value) -> String {
    // Try file path first
    for key in &["file", "path", "filePath"] {
        if let Some(val) = input.get(*key).and_then(|v| v.as_str()) {
            return truncate_message(val, 100);
        }
    }
    // Try files array
    if let Some(files) = input.get("files").and_then(|v| v.as_array()) {
        let paths: Vec<&str> = files
            .iter()
            .filter_map(|f| f.get("path").and_then(|v| v.as_str()))
            .collect();
        if !paths.is_empty() {
            return paths.join(", ");
        }
    }
    // Try query/prompt
    for key in &["query", "prompt", "why"] {
        if let Some(val) = input.get(*key).and_then(|v| v.as_str()) {
            return truncate_message(val, 100);
        }
    }
    String::new()
}

/// Try to find the project name via workspace-sessions mapping.
fn find_project_name(exec_path: &Path, chat_session_id: Option<&str>) -> Option<String> {
    // Navigate up to the kiro.kiroagent root
    let kiro_root = exec_path.ancestors().find(|p| {
        p.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n == "kiro.kiroagent")
    })?;

    let ws_sessions_dir = kiro_root.join("workspace-sessions");
    if !ws_sessions_dir.is_dir() {
        return None;
    }

    // If we have a chatSessionId, look it up in workspace session indexes
    if let Some(session_id) = chat_session_id {
        let rd = std::fs::read_dir(&ws_sessions_dir).ok()?;
        for entry in rd.flatten() {
            let ws_dir = entry.path();
            if !ws_dir.is_dir() {
                continue;
            }
            let index_path = ws_dir.join("sessions.json");
            let content = std::fs::read_to_string(&index_path).ok()?;
            let entries: Vec<KiroSessionIndex> = serde_json::from_str(&content).ok()?;

            for e in &entries {
                if e.session_id == session_id
                    && let Some(ref ws) = e.workspace_directory
                {
                    return Path::new(ws)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|s| s.to_string());
                }
            }
        }
    }

    // Fallback: try base64-decoding workspace dir names
    if let Ok(rd) = std::fs::read_dir(&ws_sessions_dir) {
        for entry in rd.flatten() {
            let ws_dir = entry.path();
            if !ws_dir.is_dir() {
                continue;
            }
            if let Some(name) = ws_dir.file_name().and_then(|n| n.to_str())
                && let Some(decoded) = base64_decode(name)
                && let Ok(s) = String::from_utf8(decoded)
            {
                return Path::new(&s)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|s| s.to_string());
            }
        }
    }

    None
}

/// Convert a millisecond epoch timestamp to an ISO 8601 UTC string.
fn ms_to_iso8601(ms: i64) -> String {
    let total_secs = ms / 1000;
    let millis = (ms % 1000).unsigned_abs();

    let mut days = total_secs / 86400;
    let day_secs = total_secs % 86400;
    let hour = day_secs / 3600;
    let min = (day_secs % 3600) / 60;
    let sec = day_secs % 60;

    // Civil date from days since epoch (Howard Hinnant's algorithm)
    days += 719468;
    let era = days / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        y, m, d, hour, min, sec, millis
    )
}

/// Simple base64 decoder (standard alphabet). Returns None on invalid input.
fn base64_decode(input: &str) -> Option<Vec<u8>> {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let input = input.trim_end_matches('=');
    let mut out = Vec::with_capacity(input.len() * 3 / 4);
    let mut buf = 0u32;
    let mut bits = 0u32;
    for &b in input.as_bytes() {
        let val = TABLE.iter().position(|&c| c == b)? as u32;
        buf = (buf << 6) | val;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper: write a file and return its path.
    fn write_file(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, content).unwrap();
        path
    }

    /// Build a minimal execution data JSON.
    fn minimal_exec_json() -> serde_json::Value {
        serde_json::json!({
            "executionId": "test-exec-001",
            "workflowType": "spec-generation",
            "status": "succeed",
            "title": "Generate Spec",
            "startTime": 1772721930147_i64,
            "endTime": 1772722211157_i64,
            "chatSessionId": "sess-001",
            "context": {
                "messages": [
                    {
                        "role": "human",
                        "entries": [
                            {"type": "text", "text": "请帮我实现导出功能"}
                        ],
                        "messageId": "msg-1"
                    },
                    {
                        "role": "bot",
                        "entries": [
                            {"type": "text", "text": "好的，让我先了解项目结构。"},
                            {"type": "toolUse", "name": "listDirectory", "args": {"path": "."}, "id": "tool-1"}
                        ],
                        "messageId": "msg-2"
                    },
                    {
                        "role": "tool",
                        "entries": [
                            {"type": "toolUseResponse", "id": "tool-1", "message": "src/\nCargo.toml"}
                        ],
                        "messageId": "msg-3"
                    },
                    {
                        "role": "bot",
                        "entries": [
                            {"type": "text", "text": "我来读取源代码。"}
                        ],
                        "messageId": "msg-4"
                    }
                ]
            },
            "actions": [
                {"type": "AgentExecutionAction", "actionType": "model", "actionState": "Success"},
                {"type": "AgentExecutionAction", "actionType": "readFiles", "actionState": "Accepted",
                 "input": {"files": [{"path": "src/main.rs"}, {"path": "Cargo.toml"}]}},
                {"type": "AgentExecutionAction", "actionType": "search", "actionState": "Accepted",
                 "input": {"query": "export function", "why": "查找导出相关代码"}},
                {"type": "AgentExecutionAction", "actionType": "create", "actionState": "Accepted",
                 "input": {"file": "src/export.rs"}},
                {"type": "AgentExecutionAction", "actionType": "say", "actionState": "Success",
                 "output": {"message": "导出功能已实现"}}
            ],
            "usageSummary": [
                {"usage": 0.75, "unit": "credit", "unitPlural": "credits"},
                {"usage": 0.25, "unit": "credit", "unitPlural": "credits", "usedTools": ["invokeSubAgent"]}
            ]
        })
    }

    /// Setup a mock Kiro directory structure with execution data files.
    fn setup_exec_dir(root: &Path, exec_files: &[(&str, &serde_json::Value)]) -> Vec<PathBuf> {
        let profile_dir = root.join("profile-hash");
        let exec_data_dir = profile_dir.join("exec-data-hash");
        std::fs::create_dir_all(&exec_data_dir).unwrap();

        let mut paths = Vec::new();
        for (name, json) in exec_files {
            let content = serde_json::to_string(json).unwrap();
            let path = write_file(&exec_data_dir, name, &content);
            paths.push(path);
        }
        paths
    }

    // ── Unit Tests: File Discovery ───────────────────────────────────────────

    #[test]
    fn discover_groups_by_chat_session_id() {
        let dir = TempDir::new().unwrap();
        let json = minimal_exec_json();
        // Two files with the same chatSessionId should be grouped into one
        setup_exec_dir(dir.path(), &[("exec1", &json), ("exec2", &json)]);

        let source = KiroSource;
        let discovered = source.discover_paths(dir.path());

        assert_eq!(discovered.len(), 1, "same chatSessionId → one session");
    }

    #[test]
    fn discover_separates_different_sessions() {
        let dir = TempDir::new().unwrap();
        let json1 = minimal_exec_json();
        let mut json2 = minimal_exec_json();
        json2["chatSessionId"] = serde_json::json!("different-session-id");
        setup_exec_dir(dir.path(), &[("exec1", &json1), ("exec2", &json2)]);

        let source = KiroSource;
        let discovered = source.discover_paths(dir.path());

        assert_eq!(
            discovered.len(),
            2,
            "different chatSessionIds → two sessions"
        );
    }

    #[test]
    fn discover_skips_source_snapshot_dirs() {
        let dir = TempDir::new().unwrap();
        let profile_dir = dir.path().join("profile-hash");

        // Create a source snapshot dir (has subdirs with source files)
        let snapshot_dir = profile_dir.join("snapshot-hash");
        std::fs::create_dir_all(snapshot_dir.join("src")).unwrap();
        std::fs::write(snapshot_dir.join("src/main.rs"), "fn main() {}").unwrap();

        // Create an execution data dir
        let exec_dir = profile_dir.join("exec-data-hash");
        std::fs::create_dir_all(&exec_dir).unwrap();
        let json = minimal_exec_json();
        std::fs::write(
            exec_dir.join("exec1"),
            serde_json::to_string(&json).unwrap(),
        )
        .unwrap();

        let source = KiroSource;
        let discovered = source.discover_paths(dir.path());

        assert_eq!(discovered.len(), 1);
    }

    #[test]
    fn discover_skips_known_dirs() {
        let dir = TempDir::new().unwrap();
        for name in &[
            "workspace-sessions",
            "default",
            "index",
            "dev_data",
            ".diffs",
        ] {
            let d = dir.path().join(name);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("data"), "{}").unwrap();
        }

        let source = KiroSource;
        assert!(source.discover_paths(dir.path()).is_empty());
    }

    #[test]
    fn discover_empty_dir() {
        let dir = TempDir::new().unwrap();
        let source = KiroSource;
        assert!(source.discover_paths(dir.path()).is_empty());
    }

    // ── Unit Tests: Transcript Parsing ───────────────────────────────────────

    #[test]
    fn parse_typical_execution() {
        let dir = TempDir::new().unwrap();
        let json = minimal_exec_json();
        let content = serde_json::to_string(&json).unwrap();
        let path = write_file(dir.path(), "exec.json", &content);

        let source = KiroSource;
        let data = source.parse_transcript(&path).unwrap();

        assert_eq!(data.source, SourceKind::Kiro);
        assert_eq!(data.tool_call_total, 4); // readFiles, search, create, say (not model)
        assert_eq!(data.user_message_count, 1);
        assert_eq!(data.assistant_message_count, 2); // two "bot" messages
        assert!(data.models.contains("spec-generation"));
        assert_eq!(
            data.first_user_message.as_deref(),
            Some("请帮我实现导出功能")
        );
        assert_eq!(data.summary.as_deref(), Some("Generate Spec"));
        assert_eq!(data.slug.as_deref(), Some("spec-generation"));
        assert!(data.duration_ms > 0);
        // Credits
        assert!((data.estimated_cost_usd - 1.0).abs() < 0.001);
    }

    #[test]
    fn parse_extracts_files_touched() {
        let dir = TempDir::new().unwrap();
        let json = minimal_exec_json();
        let content = serde_json::to_string(&json).unwrap();
        let path = write_file(dir.path(), "exec.json", &content);

        let source = KiroSource;
        let data = source.parse_transcript(&path).unwrap();

        assert!(data.files_touched.contains(&"src/export.rs".to_string()));
    }

    #[test]
    fn parse_empty_execution() {
        let dir = TempDir::new().unwrap();
        let json = serde_json::json!({"executionId": "empty"});
        let content = serde_json::to_string(&json).unwrap();
        let path = write_file(dir.path(), "empty.json", &content);

        let source = KiroSource;
        let data = source.parse_transcript(&path).unwrap();

        assert_eq!(data.tool_call_total, 0);
        assert_eq!(data.user_message_count, 0);
        assert_eq!(data.estimated_cost_usd, 0.0);
    }

    #[test]
    fn parse_invalid_json_returns_error() {
        let dir = TempDir::new().unwrap();
        let path = write_file(dir.path(), "bad.json", "not json {{{");

        let source = KiroSource;
        let err = source
            .parse_transcript(&path)
            .err()
            .expect("should be an error");
        assert!(format!("{}", err).contains("解析 JSON 失败"));
    }

    // ── Unit Tests: Conversation Parsing ─────────────────────────────────────

    #[test]
    fn parse_conversation_with_tool_calls() {
        let dir = TempDir::new().unwrap();
        let json = minimal_exec_json();
        let content = serde_json::to_string(&json).unwrap();
        let path = write_file(dir.path(), "exec.json", &content);

        let source = KiroSource;
        let turns = source.parse_conversation(&path).unwrap();

        // human, bot (text + toolUse), bot (text) — tool role is skipped
        assert!(turns.len() >= 2);
        assert_eq!(turns[0].role, ConversationRole::User);
        assert_eq!(turns[0].content, "请帮我实现导出功能");

        // Second turn is assistant with tool call
        assert_eq!(turns[1].role, ConversationRole::Assistant);
        assert!(turns[1].content.contains("了解项目结构"));
        assert!(!turns[1].tool_calls.is_empty());
        assert_eq!(turns[1].tool_calls[0].name, "listDirectory");
    }

    #[test]
    fn parse_conversation_skips_system_prompts() {
        let dir = TempDir::new().unwrap();
        let json = serde_json::json!({
            "executionId": "test",
            "context": {
                "messages": [
                    {
                        "role": "human",
                        "entries": [
                            {"type": "text", "text": "# System Prompt\nYou are an AI..."},
                            {"type": "text", "text": "<identity>\nYou are Kiro..."}
                        ]
                    },
                    {
                        "role": "bot",
                        "entries": [{"type": "text", "text": "I will follow these instructions."}]
                    },
                    {
                        "role": "human",
                        "entries": [{"type": "text", "text": "Help me code"}]
                    }
                ]
            }
        });
        let content = serde_json::to_string(&json).unwrap();
        let path = write_file(dir.path(), "exec.json", &content);

        let source = KiroSource;
        let turns = source.parse_conversation(&path).unwrap();

        // System prompt entries should be filtered
        assert!(turns.iter().all(|t| !t.content.contains("System Prompt")));
        // "Help me code" should be present
        assert!(turns.iter().any(|t| t.content.contains("Help me code")));
    }

    // ── Unit Tests: Session Merging ─────────────────────────────────────────

    #[test]
    fn parse_merges_executions_with_same_session_id() {
        let dir = TempDir::new().unwrap();
        let profile_dir = dir.path().join("profile-hash");
        let exec_dir = profile_dir.join("exec-data-hash");
        std::fs::create_dir_all(&exec_dir).unwrap();

        // Two executions with the same chatSessionId
        let exec1 = serde_json::json!({
            "executionId": "exec-1",
            "chatSessionId": "same-session",
            "startTime": 1000000,
            "endTime": 1100000,
            "context": {
                "messages": [
                    {"role": "human", "entries": [{"type": "text", "text": "First question"}]},
                    {"role": "bot", "entries": [{"type": "text", "text": "First answer"}]}
                ]
            },
            "actions": [
                {"type": "AgentExecutionAction", "actionType": "readFiles", "input": {"files": [{"path": "a.rs"}]}}
            ],
            "usageSummary": [{"usage": 0.5, "unit": "credit"}]
        });

        let exec2 = serde_json::json!({
            "executionId": "exec-2",
            "chatSessionId": "same-session",
            "startTime": 1200000,
            "endTime": 1300000,
            "context": {
                "messages": [
                    {"role": "human", "entries": [{"type": "text", "text": "Second question"}]},
                    {"role": "bot", "entries": [{"type": "text", "text": "Second answer"}]}
                ]
            },
            "actions": [
                {"type": "AgentExecutionAction", "actionType": "create", "input": {"file": "b.rs"}}
            ],
            "usageSummary": [{"usage": 0.3, "unit": "credit"}]
        });

        let path1 = write_file(&exec_dir, "exec1", &serde_json::to_string(&exec1).unwrap());
        write_file(&exec_dir, "exec2", &serde_json::to_string(&exec2).unwrap());

        let source = KiroSource;
        let data = source.parse_transcript(&path1).unwrap();

        // Should merge: 2+2 messages, 2 tool calls, 0.8 credits
        assert_eq!(data.user_message_count, 2);
        assert_eq!(data.assistant_message_count, 2);
        assert_eq!(data.tool_call_total, 2);
        assert!((data.estimated_cost_usd - 0.8).abs() < 0.001);
        // Duration spans from earliest start to latest end
        assert_eq!(data.duration_ms, 300000); // 1300000 - 1000000
        // Files from both executions
        assert!(data.files_touched.contains(&"b.rs".to_string()));
    }

    #[test]
    fn parse_conversation_merges_turns() {
        let dir = TempDir::new().unwrap();
        let profile_dir = dir.path().join("profile-hash");
        let exec_dir = profile_dir.join("exec-data-hash");
        std::fs::create_dir_all(&exec_dir).unwrap();

        let exec1 = serde_json::json!({
            "executionId": "exec-1", "chatSessionId": "same-session", "startTime": 1000,
            "context": {"messages": [
                {"role": "human", "entries": [{"type": "text", "text": "Q1"}]},
                {"role": "bot", "entries": [{"type": "text", "text": "A1"}]}
            ]}, "actions": []
        });
        let exec2 = serde_json::json!({
            "executionId": "exec-2", "chatSessionId": "same-session", "startTime": 2000,
            "context": {"messages": [
                {"role": "human", "entries": [{"type": "text", "text": "Q2"}]},
                {"role": "bot", "entries": [{"type": "text", "text": "A2"}]}
            ]}, "actions": []
        });

        let path1 = write_file(&exec_dir, "e1", &serde_json::to_string(&exec1).unwrap());
        write_file(&exec_dir, "e2", &serde_json::to_string(&exec2).unwrap());

        let source = KiroSource;
        let turns = source.parse_conversation(&path1).unwrap();

        assert_eq!(turns.len(), 4);
        assert_eq!(turns[0].content, "Q1");
        assert_eq!(turns[1].content, "A1");
        assert_eq!(turns[2].content, "Q2");
        assert_eq!(turns[3].content, "A2");
    }

    // ── Unit Tests: Batch Loading ────────────────────────────────────────────

    #[test]
    fn load_includes_sessions_with_actions() {
        let dir = TempDir::new().unwrap();
        let json = minimal_exec_json();
        let content = serde_json::to_string(&json).unwrap();
        let path = write_file(dir.path(), "exec.json", &content);

        let source = KiroSource;
        let results = source.load_transcripts(&[path]);

        assert_eq!(results.len(), 1);
    }

    #[test]
    fn load_skips_empty_executions() {
        let dir = TempDir::new().unwrap();
        let json = serde_json::json!({"executionId": "empty"});
        let content = serde_json::to_string(&json).unwrap();
        let path = write_file(dir.path(), "empty.json", &content);

        let source = KiroSource;
        assert!(source.load_transcripts(&[path]).is_empty());
    }

    #[test]
    fn load_continues_on_failure() {
        let dir = TempDir::new().unwrap();
        let json = minimal_exec_json();
        let valid = write_file(
            dir.path(),
            "valid.json",
            &serde_json::to_string(&json).unwrap(),
        );
        let bad = write_file(dir.path(), "bad.json", "not json!!!");

        let source = KiroSource;
        let results = source.load_transcripts(&[bad, valid]);

        assert_eq!(results.len(), 1);
    }

    // ── Unit Tests: Helpers ──────────────────────────────────────────────────

    #[test]
    fn identify_kiro_action_types() {
        assert_eq!(identify_tool_type("readFiles"), "file_read");
        assert_eq!(identify_tool_type("write"), "file_write");
        assert_eq!(identify_tool_type("create"), "file_write");
        assert_eq!(identify_tool_type("append"), "file_write");
        assert_eq!(identify_tool_type("search"), "search");
        assert_eq!(identify_tool_type("say"), "user_interaction");
        assert_eq!(identify_tool_type("invokeSubAgent"), "subagent");
        assert_eq!(identify_tool_type("subagentResponse"), "subagent");
        assert_eq!(identify_tool_type("preWork"), "diagnostics");
        assert_eq!(identify_tool_type("unknownAction"), "unknownAction");
    }

    #[test]
    fn ms_to_iso8601_epoch() {
        assert_eq!(ms_to_iso8601(0), "1970-01-01T00:00:00.000Z");
    }

    #[test]
    fn ms_to_iso8601_known_date() {
        let result = ms_to_iso8601(1772721930147);
        // Should be a date in 2026
        assert!(result.starts_with("2026-"));
    }

    #[test]
    fn base64_decode_workspace_path() {
        // "L1VzZXJzL21hYy9Qcm9qZWN0cy9haW5r" = "/Users/mac/Projects/aink"
        let decoded = base64_decode("L1VzZXJzL21hYy9Qcm9qZWN0cy9haW5r").unwrap();
        let s = String::from_utf8(decoded).unwrap();
        assert_eq!(s, "/Users/mac/Projects/aink");
    }

    #[test]
    fn action_summary_extracts_file_path() {
        let input = serde_json::json!({"file": "src/main.rs"});
        assert_eq!(extract_action_summary(&input), "src/main.rs");
    }

    #[test]
    fn action_summary_extracts_files_array() {
        let input = serde_json::json!({"files": [{"path": "a.rs"}, {"path": "b.rs"}]});
        assert_eq!(extract_action_summary(&input), "a.rs, b.rs");
    }

    #[test]
    fn action_summary_extracts_query() {
        let input = serde_json::json!({"query": "find exports", "why": "locate code"});
        assert_eq!(extract_action_summary(&input), "find exports");
    }
}
