//! Claude Code transcript parser.
//!
//! Handles the JSONL format produced by Claude Code, including subagent
//! discovery and merging.

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

// ── Claude Code JSONL schema ────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct TranscriptEntry {
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    message: Option<TranscriptMessage>,
    #[serde(rename = "isMeta", default)]
    is_meta: Option<bool>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    version: Option<String>,
    #[serde(rename = "gitBranch", default)]
    git_branch: Option<String>,
    #[serde(default)]
    slug: Option<String>,
    #[serde(default)]
    subtype: Option<String>,
    #[serde(rename = "durationMs", default)]
    duration_ms: Option<u64>,
    #[serde(rename = "toolUseResult", default)]
    tool_use_result: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct TranscriptMessage {
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    usage: Option<TranscriptUsage>,
    #[serde(default)]
    content: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct TranscriptUsage {
    #[serde(default)]
    input_tokens: Option<u64>,
    #[serde(default)]
    output_tokens: Option<u64>,
    #[serde(default)]
    cache_creation_input_tokens: Option<u64>,
    #[serde(default)]
    cache_read_input_tokens: Option<u64>,
}

// ── Source implementation ────────────────────────────────────────────────────

pub struct ClaudeSource;

impl TranscriptSource for ClaudeSource {
    fn kind(&self) -> SourceKind {
        SourceKind::Claude
    }

    fn discover_paths(&self, root: &Path) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if !root.is_dir() {
            return paths;
        }
        super::collect_jsonl_paths(root, &mut paths, &["subagents"]);
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
                Ok(mut data) => {
                    for sub_path in find_subagent_paths(path) {
                        match parse_transcript(&sub_path) {
                            Ok(sub_data) => merge_subagent(&mut data, &sub_data),
                            Err(e) => {
                                warn!("parse subagent {}: {}", sub_path.display(), e);
                            }
                        }
                    }
                    if data.input_tokens > 0 || data.output_tokens > 0 {
                        out.push((path.clone(), data));
                    }
                }
                Err(e) => {
                    warn!("parse_transcript {}: {}", path.display(), e);
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

    let mut acc = ParseAccumulator::new();
    let mut first_user_message: Option<String> = None;
    let mut duration_ms: u64 = 0;
    let mut turn_count: u64 = 0;
    let mut user_message_count: u64 = 0;
    let mut assistant_message_count: u64 = 0;
    let mut start_time: Option<String> = None;
    let mut end_time: Option<String> = None;
    let mut agent_version: Option<String> = None;
    let mut git_branch: Option<String> = None;
    let mut slug: Option<String> = None;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let entry: TranscriptEntry = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue,
        };

        let entry_type = entry.r#type.as_deref().unwrap_or("");

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

        if agent_version.is_none()
            && let Some(ref v) = entry.version
            && !v.is_empty()
        {
            agent_version = Some(v.clone());
        }
        if git_branch.is_none()
            && let Some(ref b) = entry.git_branch
            && !b.is_empty()
        {
            git_branch = Some(b.clone());
        }
        if slug.is_none()
            && let Some(ref s) = entry.slug
            && !s.is_empty()
        {
            slug = Some(s.clone());
        }

        if entry_type == "system"
            && entry.subtype.as_deref() == Some("turn_duration")
            && let Some(ms) = entry.duration_ms
        {
            duration_ms += ms;
            turn_count += 1;
        }

        if entry_type == "user"
            && !entry.is_meta.unwrap_or(false)
            && entry.tool_use_result.is_none()
        {
            user_message_count += 1;
        }

        let msg = match entry.message {
            Some(ref m) => m,
            None => continue,
        };

        if entry_type == "user"
            && !entry.is_meta.unwrap_or(false)
            && first_user_message.is_none()
            && entry.tool_use_result.is_none()
            && let Some(ref content) = msg.content
        {
            first_user_message = extract_first_user_message(content);
        }

        if entry_type == "assistant" {
            assistant_message_count += 1;
            acc.accumulate_assistant(msg);
        }
    }

    let slug = slug.map(|s| {
        if s.starts_with('-') {
            crate::utils::project_name::project_display_name(&s)
        } else {
            s
        }
    });

    let total: u64 = acc.tool_counts.values().sum();
    Ok(TranscriptData {
        source: SourceKind::Claude,
        input_tokens: acc.input_tokens,
        output_tokens: acc.output_tokens,
        cache_creation_tokens: acc.cache_creation,
        cache_read_tokens: acc.cache_read,
        models: acc.models,
        tool_call_total: total,
        tool_call_by_type: acc.tool_counts,
        files_touched: acc.files,
        first_user_message,
        per_model: acc.per_model,
        duration_ms,
        turn_count,
        user_message_count,
        assistant_message_count,
        start_time,
        end_time,
        agent_version,
        git_branch,
        slug,
        project_name: path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(crate::utils::project_name::project_display_name),
        estimated_cost_usd: 0.0,
    })
}

fn extract_first_user_message(content: &serde_json::Value) -> Option<String> {
    let text = match content {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(arr) => arr.iter().find_map(|item| {
            if item.get("type")?.as_str()? == "text" {
                item.get("text")?.as_str().map(|s| s.to_string())
            } else {
                None
            }
        }),
        _ => None,
    };
    text.map(|t| {
        const MAX_LEN: usize = 200;
        if t.len() > MAX_LEN {
            let end = t
                .char_indices()
                .take(MAX_LEN)
                .last()
                .map(|(i, _)| i)
                .unwrap_or(t.len());
            format!("{}...", &t[..end])
        } else {
            t
        }
    })
}

/// Mutable accumulators collected while walking transcript entries.
struct ParseAccumulator {
    input_tokens: u64,
    output_tokens: u64,
    cache_creation: u64,
    cache_read: u64,
    models: HashSet<String>,
    per_model: HashMap<String, ModelStats>,
    tool_counts: HashMap<String, u64>,
    files: Vec<String>,
    seen_files: HashSet<String>,
}

impl ParseAccumulator {
    fn new() -> Self {
        Self {
            input_tokens: 0,
            output_tokens: 0,
            cache_creation: 0,
            cache_read: 0,
            models: HashSet::new(),
            per_model: HashMap::new(),
            tool_counts: HashMap::new(),
            files: Vec::new(),
            seen_files: HashSet::new(),
        }
    }

    fn accumulate_assistant(&mut self, msg: &TranscriptMessage) {
        if let Some(ref usage) = msg.usage {
            self.input_tokens += usage.input_tokens.unwrap_or(0);
            self.output_tokens += usage.output_tokens.unwrap_or(0);
            self.cache_creation += usage.cache_creation_input_tokens.unwrap_or(0);
            self.cache_read += usage.cache_read_input_tokens.unwrap_or(0);
        }

        if let Some(model) = msg.model.as_ref()
            && model != "<synthetic>"
        {
            self.models.insert(model.to_string());
            let model_stats = self.per_model.entry(model.to_string()).or_default();
            if let Some(ref usage) = msg.usage {
                model_stats.input_tokens += usage.input_tokens.unwrap_or(0);
                model_stats.output_tokens += usage.output_tokens.unwrap_or(0);
                model_stats.cache_creation_tokens += usage.cache_creation_input_tokens.unwrap_or(0);
                model_stats.cache_read_tokens += usage.cache_read_input_tokens.unwrap_or(0);
            }
        }

        if let Some(serde_json::Value::Array(ref contents)) = msg.content {
            for item in contents {
                if item.get("type").and_then(|v| v.as_str()) == Some("tool_use") {
                    let tool_name = item
                        .get("name")
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    *self.tool_counts.entry(tool_name.to_string()).or_insert(0) += 1;

                    if let Some(model) = msg.model.as_ref()
                        && model != "<synthetic>"
                        && let Some(ms) = self.per_model.get_mut(model.as_str())
                    {
                        ms.tool_call_count += 1;
                    }

                    if matches!(tool_name, "Write" | "Edit" | "MultiEdit")
                        && let Some(input) = item.get("input")
                        && let Some(fp) = input.get("file_path").and_then(|v| v.as_str())
                        && self.seen_files.insert(fp.to_string())
                    {
                        self.files.push(fp.to_string());
                    }
                }
            }
        }
    }
}

// ── Conversation parsing ────────────────────────────────────────────────────

fn parse_conversation(path: &Path) -> color_eyre::Result<Vec<ConversationTurn>> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut turns = Vec::new();

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let entry: TranscriptEntry = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(_) => continue,
        };
        let entry_type = entry.r#type.as_deref().unwrap_or("");
        let msg = match entry.message {
            Some(ref m) => m,
            None => continue,
        };
        if entry.is_meta == Some(true) {
            continue;
        }

        if entry_type == "user" {
            if entry.tool_use_result.is_some() {
                continue;
            }
            let text = extract_text_from_content(msg.content.as_ref());
            if let Some(t) = text
                && !t.trim().is_empty()
            {
                let cleaned = strip_xml_tags(&t);
                if !cleaned.trim().is_empty() {
                    turns.push(ConversationTurn {
                        role: ConversationRole::User,
                        content: cleaned,
                        tool_calls: Vec::new(),
                        created_at: entry.timestamp.clone(),
                    });
                }
            }
        } else if entry_type == "assistant" {
            let (content, tool_calls) = format_assistant_content(msg.content.as_ref());
            if !content.trim().is_empty() || !tool_calls.is_empty() {
                turns.push(ConversationTurn {
                    role: ConversationRole::Assistant,
                    content,
                    tool_calls,
                    created_at: entry.timestamp.clone(),
                });
            }
        }
    }
    Ok(turns)
}

/// Strip XML-like tags from text, keeping inner content.
/// Turns `<tag>value</tag>` into `tag: value` for readability.
fn strip_xml_tags(text: &str) -> String {
    let mut result = String::new();
    let mut rest = text;

    while let Some(open_start) = rest.find('<') {
        let before = &rest[..open_start];
        if !before.trim().is_empty() {
            result.push_str(before.trim());
            result.push('\n');
        }

        let Some(open_end) = rest[open_start..].find('>') else {
            result.push_str(rest);
            return result;
        };
        let open_end = open_start + open_end;
        let tag_content = &rest[open_start + 1..open_end];

        if tag_content.starts_with('/') {
            rest = &rest[open_end + 1..];
            continue;
        }

        let tag_name = tag_content.split_whitespace().next().unwrap_or(tag_content);
        let close_tag = format!("</{}>", tag_name);

        if let Some(close_pos) = rest.find(&close_tag) {
            let inner = rest[open_end + 1..close_pos].trim();
            if !inner.is_empty() {
                result.push_str(&format!("{}: {}\n", tag_name, inner));
            }
            rest = &rest[close_pos + close_tag.len()..];
        } else {
            rest = &rest[open_end + 1..];
        }
    }

    let trailing = rest.trim();
    if !trailing.is_empty() {
        result.push_str(trailing);
        result.push('\n');
    }

    result.trim().to_string()
}

fn extract_text_from_content(content: Option<&serde_json::Value>) -> Option<String> {
    let content = content?;
    match content {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(arr) => {
            let parts: Vec<String> = arr
                .iter()
                .filter_map(|item| {
                    if item.get("type")?.as_str()? == "text" {
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

fn summarize_tool_input(name: &str, input: Option<&serde_json::Value>) -> String {
    let Some(obj) = input.and_then(|v| v.as_object()) else {
        return String::new();
    };

    let get_str = |key: &str| obj.get(key).and_then(|v| v.as_str()).map(|s| s.to_string());

    match name {
        "Read" | "Write" | "Edit" | "MultiEdit" => get_str("file_path").unwrap_or_default(),
        "Bash" | "Shell" => get_str("command").unwrap_or_default(),
        "Grep" | "Search" | "RipGrep" => {
            let pattern = get_str("pattern").unwrap_or_default();
            let path = get_str("path")
                .or_else(|| get_str("directory"))
                .unwrap_or_default();
            if path.is_empty() {
                pattern
            } else {
                format!("{} in {}", pattern, path)
            }
        }
        "Glob" => get_str("pattern").unwrap_or_default(),
        _ => obj
            .iter()
            .find_map(|(k, v)| v.as_str().map(|s| format!("{}: {}", k, s)))
            .unwrap_or_default(),
    }
}

fn format_assistant_content(content: Option<&serde_json::Value>) -> (String, Vec<ToolCallDetail>) {
    let arr = match content {
        Some(serde_json::Value::Array(a)) => a,
        _ => return (String::new(), Vec::new()),
    };
    let mut text_parts = Vec::new();
    let mut tool_calls = Vec::new();
    for item in arr {
        let item_type = item.get("type").and_then(|v| v.as_str());
        if item_type == Some("text") {
            if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                text_parts.push(t.to_string());
            }
        } else if item_type == Some("tool_use") {
            let name = item
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let summary = summarize_tool_input(name, item.get("input"));
            tool_calls.push(ToolCallDetail {
                name: name.to_string(),
                summary,
            });
        }
    }
    (text_parts.join("\n"), tool_calls)
}

// ── Subagent handling ───────────────────────────────────────────────────────

fn merge_subagent(parent: &mut TranscriptData, sub: &TranscriptData) {
    parent.input_tokens += sub.input_tokens;
    parent.output_tokens += sub.output_tokens;
    parent.cache_creation_tokens += sub.cache_creation_tokens;
    parent.cache_read_tokens += sub.cache_read_tokens;
    parent.duration_ms += sub.duration_ms;
    parent.turn_count += sub.turn_count;
    parent.assistant_message_count += sub.assistant_message_count;
    parent.tool_call_total += sub.tool_call_total;

    for model in &sub.models {
        parent.models.insert(model.clone());
    }
    for (tool, count) in &sub.tool_call_by_type {
        *parent.tool_call_by_type.entry(tool.clone()).or_insert(0) += count;
    }
    for (model, stats) in &sub.per_model {
        let entry = parent.per_model.entry(model.clone()).or_default();
        entry.input_tokens += stats.input_tokens;
        entry.output_tokens += stats.output_tokens;
        entry.cache_creation_tokens += stats.cache_creation_tokens;
        entry.cache_read_tokens += stats.cache_read_tokens;
        entry.tool_call_count += stats.tool_call_count;
    }
}

fn find_subagent_paths(parent_path: &Path) -> Vec<PathBuf> {
    let stem = match parent_path.file_stem() {
        Some(s) => s,
        None => return Vec::new(),
    };
    let parent_dir = match parent_path.parent() {
        Some(d) => d,
        None => return Vec::new(),
    };
    let subagents_dir = parent_dir.join(stem).join("subagents");
    if !subagents_dir.is_dir() {
        return Vec::new();
    }
    let mut paths = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&subagents_dir) {
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "jsonl") {
                paths.push(path);
            }
        }
    }
    paths
}
