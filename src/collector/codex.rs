//! OpenAI Codex CLI transcript parser.
//!
//! Handles the JSONL format produced by Codex CLI (`~/.codex/sessions/`),
//! organized in `YYYY/MM/DD/rollout-<timestamp>-<uuid>.jsonl` files.

use std::collections::{HashMap, HashSet};
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use serde::Deserialize;
use tracing::warn;

use super::source::{SourceKind, TranscriptSource};
use super::transcript::{
    ConversationRole, ConversationTurn, ModelStats, ToolCallDetail, TranscriptData,
};

// ── Codex JSONL schema ──────────────────────────────────────────────────────

/// Top-level line in a Codex rollout JSONL file.
#[derive(Debug, Deserialize)]
struct RolloutLine {
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    payload: Option<serde_json::Value>,
}

// ── Source implementation ────────────────────────────────────────────────────

pub struct CodexSource;

impl TranscriptSource for CodexSource {
    fn kind(&self) -> SourceKind {
        SourceKind::Codex
    }

    fn discover_paths(&self, root: &Path) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if !root.is_dir() {
            return paths;
        }
        super::collect_jsonl_paths(root, &mut paths, &[]);
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
        paths
    }

    fn parse_transcript(&self, path: &Path) -> color_eyre::Result<TranscriptData> {
        parse_transcript(path)
    }

    fn parse_conversation(&self, path: &Path) -> color_eyre::Result<Vec<ConversationTurn>> {
        parse_conversation(path)
    }

    fn load_transcripts(&self, paths: &[PathBuf]) -> Vec<(PathBuf, TranscriptData)> {
        let mut out = Vec::with_capacity(paths.len());
        for path in paths {
            match parse_transcript(path) {
                Ok(data) => {
                    if data.input_tokens > 0 || data.output_tokens > 0 {
                        out.push((path.clone(), data));
                    }
                }
                Err(e) => {
                    warn!("parse_transcript (codex) {}: {}", path.display(), e);
                }
            }
        }
        out
    }
}

// ── File discovery ──────────────────────────────────────────────────────────

// ── Transcript parsing ──────────────────────────────────────────────────────

fn parse_transcript(path: &Path) -> color_eyre::Result<TranscriptData> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);

    let mut input_tokens: u64 = 0;
    let mut output_tokens: u64 = 0;
    let mut models: HashSet<String> = HashSet::new();
    let mut per_model: HashMap<String, ModelStats> = HashMap::new();
    let mut tool_counts: HashMap<String, u64> = HashMap::new();
    let mut files_touched: Vec<String> = Vec::new();
    let mut seen_files: HashSet<String> = HashSet::new();
    let mut first_user_message: Option<String> = None;
    let mut user_message_count: u64 = 0;
    let mut assistant_message_count: u64 = 0;
    let mut start_time: Option<String> = None;
    let mut end_time: Option<String> = None;
    let mut agent_version: Option<String> = None;
    let mut git_branch: Option<String> = None;
    let mut slug: Option<String> = None;
    let mut current_model: Option<String> = None;
    let mut turn_count: u64 = 0;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let entry: RolloutLine = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let entry_type = entry.r#type.as_deref().unwrap_or("");
        let payload = match entry.payload {
            Some(ref p) => p,
            None => continue,
        };

        // Track time range
        if let Some(ref ts) = entry.timestamp
            && !ts.is_empty()
        {
            if start_time.is_none() || start_time.as_ref().is_some_and(|s| ts < s) {
                start_time = Some(ts.clone());
            }
            if end_time.is_none() || end_time.as_ref().is_some_and(|s| ts > s) {
                end_time = Some(ts.clone());
            }
        }

        match entry_type {
            "session_meta" => {
                if agent_version.is_none() {
                    agent_version = payload
                        .get("cli_version")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                }
                if git_branch.is_none() {
                    git_branch = payload
                        .get("git")
                        .and_then(|g| g.get("branch"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());
                }
                if slug.is_none() {
                    slug = extract_project_name_from_meta(payload);
                }
            }

            "response_item" => {
                let payload_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
                let role = payload.get("role").and_then(|v| v.as_str()).unwrap_or("");

                match payload_type {
                    "message" if role == "user" => {
                        // Skip environment_context messages (system injected)
                        let text = extract_message_text(payload);
                        if let Some(ref t) = text
                            && t.starts_with("<environment_context>")
                        {
                            continue;
                        }
                        user_message_count += 1;
                        turn_count += 1;
                        if first_user_message.is_none()
                            && let Some(t) = text
                        {
                            first_user_message = Some(truncate_str(&t, 200));
                        }
                    }
                    "message" if role == "assistant" => {
                        assistant_message_count += 1;
                    }
                    "function_call" => {
                        let tool_name = payload
                            .get("name")
                            .and_then(|v| v.as_str())
                            .unwrap_or("unknown");
                        *tool_counts.entry(tool_name.to_string()).or_insert(0) += 1;

                        // Track model tool call counts
                        if let Some(ref model) = current_model
                            && let Some(ms) = per_model.get_mut(model)
                        {
                            ms.tool_call_count += 1;
                        }

                        // Extract touched files from function_call_output
                        // For shell calls, try to extract file paths from arguments
                        extract_files_from_call(payload, &mut files_touched, &mut seen_files);
                    }
                    _ => {}
                }
            }

            "event_msg" => {
                let payload_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");

                if payload_type == "token_count"
                    && let Some(info) = payload.get("info")
                    && let Some(last) = info.get("last_token_usage")
                {
                    let inp = last
                        .get("input_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let out = last
                        .get("output_tokens")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    input_tokens += inp;
                    output_tokens += out;

                    // Accumulate per-model stats
                    if let Some(ref model) = current_model {
                        let ms = per_model.entry(model.clone()).or_default();
                        ms.input_tokens += inp;
                        ms.output_tokens += out;
                    }
                }
            }

            "turn_context" => {
                if let Some(model) = payload.get("model").and_then(|v| v.as_str())
                    && !model.is_empty()
                {
                    current_model = Some(model.to_string());
                    models.insert(model.to_string());
                }
            }

            _ => {}
        }
    }

    let duration_ms = compute_duration_ms(start_time.as_deref(), end_time.as_deref());
    let tool_call_total: u64 = tool_counts.values().sum();
    Ok(TranscriptData {
        source: SourceKind::Codex,
        input_tokens,
        output_tokens,
        cache_creation_tokens: 0,
        cache_read_tokens: 0,
        models,
        tool_call_total,
        tool_call_by_type: tool_counts,
        files_touched,
        first_user_message,
        summary: None,
        per_model,
        duration_ms,
        turn_count,
        user_message_count,
        assistant_message_count,
        start_time,
        end_time,
        agent_version,
        git_branch,
        project_name: slug.clone(),
        slug,
        estimated_cost_usd: 0.0,
    })
}

// ── Conversation parsing ────────────────────────────────────────────────────

fn parse_conversation(path: &Path) -> color_eyre::Result<Vec<ConversationTurn>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut turns: Vec<ConversationTurn> = Vec::new();
    // Buffer tool calls to attach to the preceding assistant turn
    let mut pending_tool_calls: Vec<ToolCallDetail> = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let entry: RolloutLine = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let entry_type = entry.r#type.as_deref().unwrap_or("");
        let payload = match entry.payload {
            Some(ref p) => p,
            None => continue,
        };

        if entry_type != "response_item" {
            continue;
        }

        let payload_type = payload.get("type").and_then(|v| v.as_str()).unwrap_or("");
        let role = payload.get("role").and_then(|v| v.as_str()).unwrap_or("");

        match payload_type {
            "message" if role == "user" => {
                // Flush pending tool calls to last assistant turn
                flush_tool_calls(&mut turns, &mut pending_tool_calls);

                let text = extract_message_text(payload).unwrap_or_default();
                // Skip environment_context messages
                if text.starts_with("<environment_context>") {
                    continue;
                }
                if text.trim().is_empty() {
                    continue;
                }
                turns.push(ConversationTurn {
                    role: ConversationRole::User,
                    content: text,
                    tool_calls: Vec::new(),
                    created_at: entry.timestamp.clone(),
                });
            }
            "message" if role == "assistant" => {
                // Flush pending tool calls to last assistant turn
                flush_tool_calls(&mut turns, &mut pending_tool_calls);

                let text = extract_message_text(payload).unwrap_or_default();
                if !text.trim().is_empty() {
                    turns.push(ConversationTurn {
                        role: ConversationRole::Assistant,
                        content: text,
                        tool_calls: Vec::new(),
                        created_at: entry.timestamp.clone(),
                    });
                }
            }
            "function_call" => {
                let name = payload
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let summary = summarize_function_call(payload);
                pending_tool_calls.push(ToolCallDetail { name, summary });

                // If no assistant turn exists yet, create one for tool calls
                if !turns.iter().any(|t| t.role == ConversationRole::Assistant) {
                    turns.push(ConversationTurn {
                        role: ConversationRole::Assistant,
                        content: String::new(),
                        tool_calls: Vec::new(),
                        created_at: entry.timestamp.clone(),
                    });
                }
            }
            _ => {}
        }
    }

    // Flush remaining tool calls
    flush_tool_calls(&mut turns, &mut pending_tool_calls);

    Ok(turns)
}

// ── Helper functions ────────────────────────────────────────────────────────

/// Flush pending tool calls into the last assistant turn.
fn flush_tool_calls(turns: &mut [ConversationTurn], pending: &mut Vec<ToolCallDetail>) {
    if pending.is_empty() {
        return;
    }
    // Find the last assistant turn and attach tool calls
    if let Some(last_assistant) = turns
        .iter_mut()
        .rev()
        .find(|t| t.role == ConversationRole::Assistant)
    {
        last_assistant.tool_calls.append(pending);
    } else {
        pending.clear();
    }
}

/// Extract text content from a Codex message payload.
fn extract_message_text(payload: &serde_json::Value) -> Option<String> {
    let content = payload.get("content")?;
    match content {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(arr) => {
            let parts: Vec<String> = arr
                .iter()
                .filter_map(|item| {
                    let item_type = item.get("type")?.as_str()?;
                    if item_type == "input_text"
                        || item_type == "text"
                        || item_type == "output_text"
                    {
                        item.get("text")?.as_str().map(|s| s.to_string())
                    } else {
                        None
                    }
                })
                .collect();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join("\n"))
            }
        }
        _ => None,
    }
}

/// Summarize a function_call payload into a short description.
fn summarize_function_call(payload: &serde_json::Value) -> String {
    let name = payload
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let args_str = payload
        .get("arguments")
        .and_then(|v| v.as_str())
        .unwrap_or("{}");

    let args: serde_json::Value = serde_json::from_str(args_str).unwrap_or_default();

    match name {
        "shell" => {
            // command is an array like ["bash", "-lc", "actual command"]
            if let Some(cmd) = args.get("command") {
                if let Some(arr) = cmd.as_array() {
                    // Last element is typically the actual command
                    if let Some(last) = arr.last().and_then(|v| v.as_str()) {
                        return truncate_str(last, 80);
                    }
                }
                if let Some(s) = cmd.as_str() {
                    return truncate_str(s, 80);
                }
            }
            String::new()
        }
        _ => {
            // Try to extract a meaningful first string field
            if let Some(obj) = args.as_object() {
                obj.iter()
                    .find_map(|(k, v)| {
                        v.as_str()
                            .map(|s| truncate_str(&format!("{}: {}", k, s), 80))
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            }
        }
    }
}

/// Extract file paths from function_call arguments (heuristic).
fn extract_files_from_call(
    payload: &serde_json::Value,
    files: &mut Vec<String>,
    seen: &mut HashSet<String>,
) {
    let args_str = payload
        .get("arguments")
        .and_then(|v| v.as_str())
        .unwrap_or("{}");
    let args: serde_json::Value = serde_json::from_str(args_str).unwrap_or_default();

    // Look for file_path field in arguments
    if let Some(fp) = args.get("file_path").and_then(|v| v.as_str())
        && seen.insert(fp.to_string())
    {
        files.push(fp.to_string());
    }
}

/// Extract a project name from session_meta payload.
///
/// Strategy:
/// 1. Try `git.repository_url` (e.g. `git@host:org/repo.git` → `repo`)
/// 2. Fallback: find the project root component from `cwd` by looking for
///    the first directory after a known "projects" marker, then take the next
///    1–2 components.
/// 3. Last resort: last path component of `cwd`.
fn extract_project_name_from_meta(payload: &serde_json::Value) -> Option<String> {
    // 1. Try git repository URL
    if let Some(url) = payload
        .get("git")
        .and_then(|g| g.get("repository_url"))
        .and_then(|v| v.as_str())
        && let Some(name) = repo_name_from_url(url)
    {
        return Some(name);
    }

    // 2. Smart cwd extraction
    payload
        .get("cwd")
        .and_then(|v| v.as_str())
        .map(extract_project_from_cwd)
}

/// Extract repository name from a git remote URL.
///
/// Handles `git@host:org/repo.git`, `https://host/org/repo.git`, `ssh://…/repo.git`.
fn repo_name_from_url(url: &str) -> Option<String> {
    // Split on '/' or ':' and take the last segment
    let name = url.rsplit('/').next().or_else(|| url.rsplit(':').next())?;
    let name = name.strip_suffix(".git").unwrap_or(name);
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// Known parent directory names that typically sit above project roots.
const PROJECT_MARKERS: &[&str] = &[
    "projects",
    "repos",
    "repositories",
    "workspace",
    "workspaces",
];

/// Extract a project name from a `cwd` path.
///
/// Looks for the first component after a "projects marker" directory and takes
/// the next 1–2 meaningful components (e.g. `org/repo`). Falls back to the
/// last component if no marker is found.
fn extract_project_from_cwd(cwd: &str) -> String {
    let components: Vec<&str> = Path::new(cwd)
        .components()
        .filter_map(|c| c.as_os_str().to_str())
        .collect();

    // Find the first "projects marker" and take the next component(s)
    for (i, comp) in components.iter().enumerate() {
        if PROJECT_MARKERS.iter().any(|m| comp.eq_ignore_ascii_case(m)) {
            // Return up to 2 components after the marker (org/repo style)
            let rest: Vec<&str> = components[i + 1..].iter().copied().take(2).collect();
            if !rest.is_empty() {
                return rest.join("/");
            }
        }
    }

    // Fallback: last component
    components
        .last()
        .map(|s| s.to_string())
        .unwrap_or_else(|| cwd.to_string())
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        let end = s
            .char_indices()
            .take(max_len)
            .last()
            .map(|(i, _)| i)
            .unwrap_or(s.len());
        format!("{}...", &s[..end])
    }
}

/// Compute duration in milliseconds between two ISO 8601 timestamps.
///
/// Parses a subset of RFC 3339 (`YYYY-MM-DDTHH:MM:SS.fffZ`) without external
/// crate dependencies.
fn compute_duration_ms(start: Option<&str>, end: Option<&str>) -> u64 {
    let (Some(s), Some(e)) = (start, end) else {
        return 0;
    };

    fn parse_epoch_ms(ts: &str) -> Option<i64> {
        // Expected: "2025-10-04T03:18:48.171Z" or "2025-10-04T03:18:48Z"
        let ts = ts.trim_end_matches('Z');
        let (date_part, time_part) = ts.split_once('T')?;
        let mut date_iter = date_part.split('-');
        let year: i64 = date_iter.next()?.parse().ok()?;
        let month: i64 = date_iter.next()?.parse().ok()?;
        let day: i64 = date_iter.next()?.parse().ok()?;

        let (time_sec, millis) = if let Some((sec_part, ms_part)) = time_part.split_once('.') {
            let ms: i64 = ms_part.get(..3).unwrap_or(ms_part).parse().unwrap_or(0);
            (sec_part, ms)
        } else {
            (time_part, 0i64)
        };

        let mut time_iter = time_sec.split(':');
        let hour: i64 = time_iter.next()?.parse().ok()?;
        let min: i64 = time_iter.next()?.parse().ok()?;
        let sec: i64 = time_iter.next()?.parse().ok()?;

        // Days from year 0 to Jan 1 of `year` (simplified, ignoring leap seconds)
        let y = if month <= 2 { year - 1 } else { year };
        let m = if month <= 2 { month + 9 } else { month - 3 };
        let days = 365 * y + y / 4 - y / 100 + y / 400 + (m * 306 + 5) / 10 + day - 1 - 719468; // epoch offset

        Some(((days * 86400 + hour * 3600 + min * 60 + sec) * 1000) + millis)
    }

    match (parse_epoch_ms(s), parse_epoch_ms(e)) {
        (Some(start_ms), Some(end_ms)) => (end_ms - start_ms).max(0) as u64,
        _ => 0,
    }
}
