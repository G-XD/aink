//! Cursor IDE transcript parser.
//!
//! Parses Cursor's SQLite-based session storage (`state.vscdb`) to extract
//! conversation data, token usage, tool calls, and other session statistics.
//!
//! ## Storage layout
//!
//! All AI conversation data lives in a global SQLite database at
//! `~/Library/Application Support/Cursor/User/globalStorage/state.vscdb`.
//! Individual messages ("bubbles") are keyed `bubbleId:<composerId>:<bubbleId>`
//! in the `cursorDiskKV` table; session metadata is `composerData:<id>`.
//!
//! Virtual paths `<root>/sessions/<project_slug>/<composerId>` are used so
//! the rest of the application (display names, session lists) works unchanged.
//!
//! ## Performance
//!
//! The DB can exceed 500 MB with ~50 000 rows.  Three levels of optimization:
//!
//! 1. **Range queries** (`key >= ? AND key < ?`) for proper index use.
//! 2. **`json_extract` in SQL** — SQLite's C JSON parser extracts the 7
//!    scalar fields we need per bubble, cutting FFI transfer from ~200 MB
//!    to ~3 MB and eliminating Rust-side serde overhead.
//! 3. **Parallel range scan** — composer IDs are sorted and split into
//!    contiguous B-tree ranges; each thread scans its range with a
//!    dedicated connection, preserving I/O locality while parallelizing
//!    both disk reads and JSON parsing.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use serde::Deserialize;
use tiktoken_rs::CoreBPE;
use tracing::{info, warn};

use super::source::{SourceKind, TranscriptSource};
use super::transcript::{
    ConversationRole, ConversationTurn, ModelStats, ToolCallDetail, TranscriptData,
};

// ── Serde types (used only by detail-view paths, not the hot bulk load) ─────

#[derive(Debug, Deserialize)]
struct WorkspaceJson {
    folder: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WorkspaceComposerData {
    #[serde(rename = "allComposers", default)]
    all_composers: Vec<WorkspaceComposerEntry>,
}

#[derive(Debug, Deserialize)]
struct WorkspaceComposerEntry {
    #[serde(rename = "composerId")]
    composer_id: String,
}

#[derive(Clone, Debug)]
struct WorkspaceInfo {
    project_slug: String,
}

#[derive(Debug, Deserialize)]
struct Bubble {
    #[serde(rename = "type", default)]
    bubble_type: i32,
    #[serde(default)]
    text: String,
    #[serde(rename = "toolFormerData", default)]
    tool_former_data: Option<BubbleToolFormerData>,
    #[serde(rename = "createdAt", default)]
    created_at: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BubbleToolFormerData {
    #[serde(default)]
    name: Option<String>,
    #[serde(rename = "rawArgs", default)]
    raw_args: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BubbleHeader {
    #[serde(rename = "bubbleId")]
    bubble_id: String,
}

#[derive(Debug, Deserialize)]
struct ComposerData {
    #[serde(rename = "fullConversationHeadersOnly", default)]
    full_conversation_headers_only: Vec<BubbleHeader>,
}

const BUBBLE_TYPE_USER: i64 = 1;
const BUBBLE_TYPE_ASSISTANT: i64 = 2;

// ── Source implementation ───────────────────────────────────────────────────

pub struct CursorSource;

impl TranscriptSource for CursorSource {
    fn kind(&self) -> SourceKind {
        SourceKind::Cursor
    }

    /// Discover all Cursor sessions.
    ///
    /// Uses `json_extract` to pull only `createdAt` from each composerData
    /// entry (~8 bytes per row instead of ~4 KB).
    fn discover_paths(&self, root: &Path) -> Vec<PathBuf> {
        let t_total = std::time::Instant::now();
        let db_path = global_db_path(root);
        if !db_path.exists() {
            return Vec::new();
        }

        let t0 = std::time::Instant::now();
        let conn = match open_db(&db_path) {
            Ok(c) => c,
            Err(e) => {
                warn!("open cursor global db {}: {}", db_path.display(), e);
                return Vec::new();
            }
        };
        info!("[cursor discover] open_db: {:?}", t0.elapsed());

        let t0 = std::time::Instant::now();
        let ws_mapping = build_workspace_mapping(root);
        info!(
            "[cursor discover] build_workspace_mapping ({} entries): {:?}",
            ws_mapping.len(),
            t0.elapsed()
        );

        // Pre-filter: only keep sessions that have at least one conversation
        // bubble. json_array_length returns NULL for non-arrays / missing keys,
        // and COALESCE turns that into 0.
        let mut stmt = match conn.prepare(
            "SELECT key, \
                    json_extract(value, '$.createdAt'), \
                    COALESCE(json_array_length(json_extract(value, '$.fullConversationHeadersOnly')), 0) \
             FROM cursorDiskKV \
             WHERE key >= 'composerData:' AND key < 'composerData;'",
        ) {
            Ok(s) => s,
            Err(e) => {
                warn!("prepare composerData query: {}", e);
                return Vec::new();
            }
        };

        let mut sessions: Vec<(PathBuf, u64)> = Vec::new();
        let mut skipped = 0u32;

        let rows = stmt.query_map([], |row| {
            let key: String = row.get(0)?;
            let created_at: Option<i64> = row.get(1)?;
            let bubble_count: i64 = row.get(2)?;
            Ok((key, created_at.unwrap_or(0) as u64, bubble_count))
        });

        if let Ok(rows) = rows {
            for row in rows.flatten() {
                let (key, created_at, bubble_count) = row;
                let composer_id = match key.strip_prefix("composerData:") {
                    Some(id) if !id.is_empty() => id,
                    _ => continue,
                };

                if bubble_count == 0 {
                    skipped += 1;
                    continue;
                }

                let project_slug = ws_mapping
                    .get(composer_id)
                    .map(|info| info.project_slug.as_str())
                    .unwrap_or("cursor");

                sessions.push((virtual_path(root, project_slug, composer_id), created_at));
            }
        }
        info!("[cursor discover] skipped {} empty sessions", skipped);

        sessions.sort_by(|a, b| b.1.cmp(&a.1));
        info!(
            "[cursor discover] total: {:?} ({} sessions)",
            t_total.elapsed(),
            sessions.len()
        );
        sessions.into_iter().map(|(p, _)| p).collect()
    }

    fn parse_transcript(&self, path: &Path) -> color_eyre::Result<TranscriptData> {
        let composer_id = extract_composer_id(path)
            .ok_or_else(|| color_eyre::eyre::eyre!("invalid cursor path: no composer id"))?;
        let root = extract_root(path)
            .ok_or_else(|| color_eyre::eyre::eyre!("invalid cursor path: cannot derive root"))?;

        let conn = open_db(&global_db_path(root))?;
        let lo = format!("bubbleId:{}:", composer_id);
        let hi = format!("bubbleId:{};", composer_id);

        let mut stmt = conn.prepare(
            "SELECT \
               json_extract(value, '$.type'), \
               COALESCE(json_extract(value, '$.tokenCount.inputTokens'), 0), \
               COALESCE(json_extract(value, '$.tokenCount.outputTokens'), 0), \
               json_extract(value, '$.modelInfo.modelName'), \
               json_extract(value, '$.toolFormerData.name'), \
               json_extract(value, '$.createdAt'), \
               COALESCE(json_extract(value, '$.text'), '') \
             FROM cursorDiskKV \
             WHERE key >= ?1 AND key < ?2",
        )?;

        let bpe = tiktoken_rs::cl100k_base().expect("failed to load cl100k_base");
        let mut acc = LightAccumulator::default();
        let rows = stmt.query_map(rusqlite::params![lo, hi], |row| {
            let text: String = row.get::<_, Option<String>>(6)?.unwrap_or_default();
            let text_len = text.len() as u64;
            Ok(ExtractedBubble {
                key: String::new(),
                bubble_type: row.get::<_, Option<i64>>(0)?.unwrap_or(0),
                input_tokens: row.get::<_, i64>(1)? as u64,
                output_tokens: row.get::<_, i64>(2)? as u64,
                model_name: row.get(3)?,
                tool_name: row.get(4)?,
                created_at: row.get(5)?,
                text,
                text_len,
            })
        })?;

        for row in rows.flatten() {
            acc.accumulate(&row, &bpe);
        }

        let slug = extract_slug(path).map(|s| s.to_string());
        Ok(acc.into_transcript_data(slug))
    }

    fn parse_conversation(&self, path: &Path) -> color_eyre::Result<Vec<ConversationTurn>> {
        let composer_id = extract_composer_id(path)
            .ok_or_else(|| color_eyre::eyre::eyre!("invalid cursor path: no composer id"))?;
        let root = extract_root(path)
            .ok_or_else(|| color_eyre::eyre::eyre!("invalid cursor path: cannot derive root"))?;

        let conn = open_db(&global_db_path(root))?;
        parse_conversation_from_db(&conn, composer_id)
    }

    /// Parallel bulk scan: sort composer IDs, split into contiguous B-tree
    /// ranges, and let each thread do one sequential range query.
    ///
    /// Compared to per-session queries (551 random seeks), each thread
    /// scans a contiguous slice of the B-tree — preserving the sequential
    /// I/O locality of a bulk scan while parallelizing the work.
    fn load_transcripts(&self, paths: &[PathBuf]) -> Vec<(PathBuf, TranscriptData)> {
        let t_total = std::time::Instant::now();
        if paths.is_empty() {
            return Vec::new();
        }

        let root = match paths.first().and_then(|p| extract_root(p)) {
            Some(r) => r,
            None => return Vec::new(),
        };

        let db_path = global_db_path(root);

        // Build (composerId, path_index) sorted by composerId so chunks
        // correspond to contiguous key ranges in the B-tree.
        let mut cid_entries: Vec<(&str, usize)> = paths
            .iter()
            .enumerate()
            .filter_map(|(i, p)| Some((extract_composer_id(p)?, i)))
            .collect();
        cid_entries.sort_unstable_by_key(|(cid, _)| *cid);

        let n_threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
            .min(8)
            .min(cid_entries.len());
        let chunk_size = cid_entries.len().div_ceil(n_threads);

        let bpe = tiktoken_rs::cl100k_base().expect("failed to load cl100k_base");

        let results: Vec<Vec<(usize, LightAccumulator)>> = std::thread::scope(|s| {
            let handles: Vec<_> = cid_entries
                .chunks(chunk_size)
                .map(|chunk| {
                    let db = &db_path;
                    let bpe = &bpe;
                    s.spawn(move || scan_range(db, chunk, bpe))
                })
                .collect();

            handles
                .into_iter()
                .map(|h| h.join().unwrap_or_default())
                .collect()
        });

        let mut out = Vec::with_capacity(paths.len());
        for batch in results {
            for (idx, acc) in batch {
                let path = &paths[idx];
                let slug = extract_slug(path).map(|s| s.to_string());
                let data = acc.into_transcript_data(slug);
                if data.user_message_count > 0
                    || data.assistant_message_count > 0
                    || data.input_tokens > 0
                {
                    out.push((path.clone(), data));
                }
            }
        }

        backfill_from_composer_data(&db_path, &mut out);

        info!(
            "[cursor load] {} threads, {} sessions → {} results: {:?}",
            n_threads,
            paths.len(),
            out.len(),
            t_total.elapsed()
        );
        out
    }
}

// ── Extracted row (no serde, no JSON, just SQLite scalars) ──────────────────

struct ExtractedBubble {
    key: String,
    bubble_type: i64,
    input_tokens: u64,
    output_tokens: u64,
    model_name: Option<String>,
    tool_name: Option<String>,
    created_at: Option<String>,
    text: String,
    text_len: u64,
}

// ── Light accumulator for bulk load (no full JSON parsing) ──────────────────

#[derive(Default)]
struct LightAccumulator {
    db_input_tokens: u64,
    db_output_tokens: u64,
    tiktoken_input_tokens: u64,
    tiktoken_output_tokens: u64,
    estimated_input_tokens: u64,
    estimated_output_tokens: u64,
    models: HashSet<String>,
    per_model: HashMap<String, ModelStats>,
    tool_counts: HashMap<String, u64>,
    first_user_message: Option<String>,
    user_message_count: u64,
    assistant_message_count: u64,
    start_time: Option<String>,
    end_time: Option<String>,
}

impl LightAccumulator {
    fn accumulate(&mut self, row: &ExtractedBubble, bpe: &CoreBPE) {
        if let Some(ref ts) = row.created_at {
            if self.start_time.is_none() || self.start_time.as_ref().is_some_and(|s| ts < s) {
                self.start_time = Some(ts.clone());
            }
            if self.end_time.is_none() || self.end_time.as_ref().is_some_and(|s| ts > s) {
                self.end_time = Some(ts.clone());
            }
        }

        match row.bubble_type {
            BUBBLE_TYPE_USER => {
                self.user_message_count += 1;
                self.estimated_input_tokens += row.text_len / 4;
                if !row.text.is_empty() {
                    self.tiktoken_input_tokens +=
                        bpe.encode_with_special_tokens(&row.text).len() as u64;
                    if self.first_user_message.is_none() {
                        self.first_user_message = Some(truncate_to(&row.text, 200));
                    }
                }
            }
            BUBBLE_TYPE_ASSISTANT => {
                self.db_input_tokens += row.input_tokens;
                self.db_output_tokens += row.output_tokens;
                self.estimated_output_tokens += row.text_len / 4;
                if row.input_tokens == 0 && row.output_tokens == 0 && !row.text.is_empty() {
                    self.tiktoken_output_tokens +=
                        bpe.encode_with_special_tokens(&row.text).len() as u64;
                }

                let model_name = row.model_name.as_deref().filter(|n| !n.is_empty());

                if let Some(name) = model_name {
                    self.models.insert(name.to_string());
                    let ms = self.per_model.entry(name.to_string()).or_default();
                    ms.input_tokens += row.input_tokens;
                    ms.output_tokens += row.output_tokens;
                }

                if let Some(ref tool) = row.tool_name {
                    *self.tool_counts.entry(tool.clone()).or_insert(0) += 1;
                    if let Some(name) = model_name
                        && let Some(ms) = self.per_model.get_mut(name)
                    {
                        ms.tool_call_count += 1;
                    }
                } else {
                    self.assistant_message_count += 1;
                }
            }
            _ => {}
        }
    }

    fn into_transcript_data(self, slug: Option<String>) -> TranscriptData {
        let tool_call_total: u64 = self.tool_counts.values().sum();
        let duration_ms = compute_duration_from_iso(&self.start_time, &self.end_time);

        let input_tokens = if self.db_input_tokens > 0 {
            self.db_input_tokens
        } else if self.tiktoken_input_tokens > 0 {
            self.tiktoken_input_tokens
        } else {
            self.estimated_input_tokens
        };
        let output_tokens = if self.db_output_tokens > 0 {
            self.db_output_tokens
        } else if self.tiktoken_output_tokens > 0 {
            self.tiktoken_output_tokens
        } else {
            self.estimated_output_tokens
        };

        TranscriptData {
            source: SourceKind::Cursor,
            input_tokens,
            output_tokens,
            cache_creation_tokens: 0,
            cache_read_tokens: 0,
            models: self.models,
            tool_call_total,
            tool_call_by_type: self.tool_counts,
            files_touched: Vec::new(),
            first_user_message: self.first_user_message,
            per_model: self.per_model,
            duration_ms,
            turn_count: self.user_message_count,
            user_message_count: self.user_message_count,
            assistant_message_count: self.assistant_message_count,
            start_time: self.start_time,
            end_time: self.end_time,
            agent_version: None,
            git_branch: None,
            project_name: slug.clone(),
            slug,
            estimated_cost_usd: 0.0,
        }
    }
}

// ── SQLite helpers ──────────────────────────────────────────────────────────

/// Enrich session data from `composerData` entries in the DB:
/// - Model: always merge `modelConfig.modelName` into the session set (Cursor only stores
///   the current selection here, not history; per-bubble `modelInfo.modelName` when present
///   gives per-message model, so together we show bubble-derived models + current selection).
/// - Changed file list (from `originalFileStates` keys)
fn backfill_from_composer_data(db_path: &Path, sessions: &mut [(PathBuf, TranscriptData)]) {
    if sessions.is_empty() {
        return;
    }

    let conn = match open_db(db_path) {
        Ok(c) => c,
        Err(e) => {
            warn!("backfill_composer: open db: {}", e);
            return;
        }
    };

    let mut cid_to_idx: HashMap<String, usize> = HashMap::with_capacity(sessions.len());
    for (i, (path, _)) in sessions.iter().enumerate() {
        if let Some(cid) = extract_composer_id(path) {
            cid_to_idx.insert(cid.to_string(), i);
        }
    }

    let mut stmt = match conn.prepare(
        "SELECT c.key, \
                json_extract(c.value, '$.modelConfig.modelName'), \
                j.key \
         FROM cursorDiskKV AS c \
         LEFT JOIN json_each(json_extract(c.value, '$.originalFileStates')) AS j \
         WHERE c.key >= 'composerData:' AND c.key < 'composerData;'",
    ) {
        Ok(s) => s,
        Err(e) => {
            warn!("backfill_composer: prepare: {}", e);
            return;
        }
    };

    let rows = match stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<String>>(1)?,
            row.get::<_, Option<String>>(2)?,
        ))
    }) {
        Ok(r) => r,
        Err(e) => {
            warn!("backfill_composer: query: {}", e);
            return;
        }
    };

    for row in rows.flatten() {
        let (key, model, file_key) = row;
        let cid = match key.strip_prefix("composerData:") {
            Some(id) => id,
            None => continue,
        };
        let idx = match cid_to_idx.get(cid) {
            Some(&i) => i,
            None => continue,
        };

        let (_, data) = &mut sessions[idx];
        if let Some(name) = model.filter(|s| !s.is_empty()) {
            data.models.insert(name.clone());
            data.per_model.entry(name).or_default();
        }
        if let Some(uri) = file_key {
            let path = uri.strip_prefix("file://").unwrap_or(&uri);
            data.files_touched.push(path.to_string());
        }
    }
}

fn global_db_path(root: &Path) -> PathBuf {
    root.join("globalStorage").join("state.vscdb")
}

fn open_db(db_path: &Path) -> color_eyre::Result<Connection> {
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )?;
    conn.pragma_update(None, "mmap_size", 256 * 1024 * 1024)?;
    conn.pragma_update(None, "cache_size", -64000)?;
    Ok(conn)
}

/// Scan a contiguous B-tree range covering all sessions in `entries`.
///
/// `entries` must be sorted by composerId.  We issue a single range query
/// from the first composerId to just past the last, then route each row
/// to the correct accumulator via a HashMap lookup on the composerId
/// extracted from the key.
fn scan_range(
    db_path: &Path,
    entries: &[(&str, usize)],
    bpe: &CoreBPE,
) -> Vec<(usize, LightAccumulator)> {
    if entries.is_empty() {
        return Vec::new();
    }

    let conn = match open_db(db_path) {
        Ok(c) => c,
        Err(e) => {
            warn!("cursor scan_range: open db: {}", e);
            return Vec::new();
        }
    };

    let first_cid = entries.first().unwrap().0;
    let last_cid = entries.last().unwrap().0;
    let lo = format!("bubbleId:{}:", first_cid);
    let hi = format!("bubbleId:{};", last_cid);

    let wanted: HashMap<&str, usize> = entries.iter().copied().collect();
    let mut accumulators: HashMap<usize, LightAccumulator> = HashMap::new();

    let mut stmt = match conn.prepare(
        "SELECT \
           key, \
           json_extract(value, '$.type'), \
           COALESCE(json_extract(value, '$.tokenCount.inputTokens'), 0), \
           COALESCE(json_extract(value, '$.tokenCount.outputTokens'), 0), \
           json_extract(value, '$.modelInfo.modelName'), \
           json_extract(value, '$.toolFormerData.name'), \
           json_extract(value, '$.createdAt'), \
           CASE WHEN COALESCE(json_extract(value, '$.tokenCount.inputTokens'), 0) = 0 \
                 AND COALESCE(json_extract(value, '$.tokenCount.outputTokens'), 0) = 0 \
                THEN COALESCE(json_extract(value, '$.text'), '') \
                ELSE '' END, \
           LENGTH(COALESCE(json_extract(value, '$.text'), '')) \
         FROM cursorDiskKV \
         WHERE key >= ?1 AND key < ?2",
    ) {
        Ok(s) => s,
        Err(e) => {
            warn!("cursor scan_range: prepare: {}", e);
            return Vec::new();
        }
    };

    let rows = match stmt.query_map(rusqlite::params![lo, hi], |row| {
        Ok(ExtractedBubble {
            key: row.get(0)?,
            bubble_type: row.get::<_, Option<i64>>(1)?.unwrap_or(0),
            input_tokens: row.get::<_, i64>(2)? as u64,
            output_tokens: row.get::<_, i64>(3)? as u64,
            model_name: row.get(4)?,
            tool_name: row.get(5)?,
            created_at: row.get(6)?,
            text: row.get::<_, Option<String>>(7)?.unwrap_or_default(),
            text_len: row.get::<_, i64>(8).unwrap_or(0) as u64,
        })
    }) {
        Ok(r) => r,
        Err(e) => {
            warn!("cursor scan_range: query: {}", e);
            return Vec::new();
        }
    };

    for row in rows.flatten() {
        let after_prefix = match row.key.strip_prefix("bubbleId:") {
            Some(rest) => rest,
            None => continue,
        };
        let composer_id = match after_prefix.find(':') {
            Some(pos) => &after_prefix[..pos],
            None => continue,
        };
        if let Some(&idx) = wanted.get(composer_id) {
            accumulators.entry(idx).or_default().accumulate(&row, bpe);
        }
    }

    accumulators.into_iter().collect()
}

// ── Virtual path encoding/decoding ─────────────────────────────────────────

fn virtual_path(root: &Path, project_slug: &str, composer_id: &str) -> PathBuf {
    root.join("sessions").join(project_slug).join(composer_id)
}

fn extract_composer_id(path: &Path) -> Option<&str> {
    path.file_name()?.to_str()
}

/// `<root>/sessions/<slug>/<composerId>` → `<root>`
fn extract_root(path: &Path) -> Option<&Path> {
    path.parent()?.parent()?.parent()
}

/// `<root>/sessions/<slug>/<composerId>` → `<slug>`
fn extract_slug(path: &Path) -> Option<&str> {
    path.parent()?.file_name()?.to_str()
}

// ── Workspace mapping (used by discover_paths and detail-view paths) ────────

/// Scan all workspace directories **in parallel** to build the mapping.
///
/// Each workspace has its own small SQLite database — opening them
/// concurrently via `std::thread::scope` turns ~400 ms sequential I/O
/// into ~10 ms wall-clock time.
fn build_workspace_mapping(root: &Path) -> HashMap<String, WorkspaceInfo> {
    let ws_root = root.join("workspaceStorage");

    let dirs: Vec<PathBuf> = match std::fs::read_dir(&ws_root) {
        Ok(rd) => rd
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect(),
        Err(_) => return HashMap::new(),
    };

    let chunks: Vec<Vec<(String, WorkspaceInfo)>> = std::thread::scope(|s| {
        let handles: Vec<_> = dirs
            .iter()
            .map(|dir| s.spawn(|| scan_single_workspace(dir)))
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().unwrap_or_default())
            .collect()
    });

    let mut mapping = HashMap::with_capacity(chunks.iter().map(|c| c.len()).sum());
    for chunk in chunks {
        for (id, info) in chunk {
            mapping.insert(id, info);
        }
    }
    mapping
}

fn scan_single_workspace(ws_dir: &Path) -> Vec<(String, WorkspaceInfo)> {
    let project_slug = read_workspace_slug(ws_dir);

    let ws_db_path = ws_dir.join("state.vscdb");
    if !ws_db_path.exists() {
        return Vec::new();
    }

    let conn = match Connection::open_with_flags(
        &ws_db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let composer_json: Option<String> = conn
        .query_row(
            "SELECT value FROM ItemTable WHERE key = 'composer.composerData'",
            [],
            |row| row.get(0),
        )
        .ok();

    let mut out = Vec::new();
    if let Some(json) = composer_json
        && let Ok(data) = serde_json::from_str::<WorkspaceComposerData>(&json)
    {
        for entry in data.all_composers {
            out.push((
                entry.composer_id,
                WorkspaceInfo {
                    project_slug: project_slug.clone(),
                },
            ));
        }
    }
    out
}

// ── Conversation parsing ────────────────────────────────────────────────────

fn parse_conversation_from_db(
    conn: &Connection,
    composer_id: &str,
) -> color_eyre::Result<Vec<ConversationTurn>> {
    let composer_json: String = conn.query_row(
        "SELECT value FROM cursorDiskKV WHERE key = ?1",
        [format!("composerData:{}", composer_id)],
        |row| row.get(0),
    )?;

    let composer: ComposerData = serde_json::from_str(&composer_json)?;
    let headers = &composer.full_conversation_headers_only;

    if headers.is_empty() {
        return Ok(Vec::new());
    }

    let mut turns = Vec::new();
    let mut pending_text = String::new();
    let mut pending_tools: Vec<ToolCallDetail> = Vec::new();
    let mut pending_created_at: Option<String> = None;

    for header in headers {
        let key = format!("bubbleId:{}:{}", composer_id, header.bubble_id);
        let value: String = match conn.query_row(
            "SELECT value FROM cursorDiskKV WHERE key = ?1",
            [&key],
            |row| row.get(0),
        ) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let bubble: Bubble = match serde_json::from_str(&value) {
            Ok(b) => b,
            Err(_) => continue,
        };

        match bubble.bubble_type as i64 {
            BUBBLE_TYPE_USER => {
                flush_assistant_turn(
                    &mut turns,
                    &mut pending_text,
                    &mut pending_tools,
                    std::mem::take(&mut pending_created_at),
                );
                let text = bubble.text.trim().to_string();
                if !text.is_empty() {
                    turns.push(ConversationTurn {
                        role: ConversationRole::User,
                        content: text,
                        tool_calls: Vec::new(),
                        created_at: bubble.created_at.clone(),
                    });
                }
            }
            BUBBLE_TYPE_ASSISTANT => {
                if let Some(ref tfd) = bubble.tool_former_data {
                    pending_created_at = bubble.created_at.clone();
                    let name = tfd.name.as_deref().unwrap_or("unknown").to_string();
                    let summary = tfd
                        .raw_args
                        .as_deref()
                        .map(|args| summarize_tool_args(&name, args))
                        .unwrap_or_default();
                    pending_tools.push(ToolCallDetail { name, summary });
                } else if !bubble.text.trim().is_empty() {
                    flush_assistant_turn(
                        &mut turns,
                        &mut pending_text,
                        &mut pending_tools,
                        std::mem::take(&mut pending_created_at),
                    );
                    pending_text = bubble.text.trim().to_string();
                    pending_created_at = bubble.created_at.clone();
                }
            }
            _ => {}
        }
    }

    flush_assistant_turn(
        &mut turns,
        &mut pending_text,
        &mut pending_tools,
        pending_created_at,
    );
    Ok(turns)
}

fn flush_assistant_turn(
    turns: &mut Vec<ConversationTurn>,
    text: &mut String,
    tools: &mut Vec<ToolCallDetail>,
    created_at: Option<String>,
) {
    if text.is_empty() && tools.is_empty() {
        return;
    }
    turns.push(ConversationTurn {
        role: ConversationRole::Assistant,
        content: std::mem::take(text),
        tool_calls: std::mem::take(tools),
        created_at,
    });
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn summarize_tool_args(tool_name: &str, raw_args: &str) -> String {
    let v: serde_json::Value = match serde_json::from_str(raw_args) {
        Ok(v) => v,
        Err(_) => return String::new(),
    };
    let obj = match v.as_object() {
        Some(o) => o,
        None => return String::new(),
    };
    let get_str = |key: &str| obj.get(key).and_then(|v| v.as_str()).map(|s| s.to_string());

    let raw = match tool_name {
        s if s.contains("read_file") || s.contains("write") || s.contains("edit") => {
            get_str("file_path")
                .or_else(|| get_str("filePath"))
                .or_else(|| get_str("path"))
                .unwrap_or_default()
        }
        s if s.contains("terminal") || s.contains("shell") || s.contains("command") => {
            get_str("command").unwrap_or_default()
        }
        s if s.contains("grep") || s.contains("search") => {
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
        _ => obj
            .iter()
            .find_map(|(k, v)| v.as_str().map(|s| format!("{}: {}", k, s)))
            .unwrap_or_default(),
    };
    raw
}

fn truncate_to(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        return s.to_string();
    }
    let end = s
        .char_indices()
        .take(max_len)
        .last()
        .map(|(i, _)| i)
        .unwrap_or(s.len());
    format!("{}...", &s[..end])
}

/// Approximate duration from ISO 8601 timestamps by parsing the first 19
/// chars (`YYYY-MM-DDTHH:MM:SS`) to avoid pulling in a datetime crate.
fn compute_duration_from_iso(start: &Option<String>, end: &Option<String>) -> u64 {
    let (Some(s), Some(e)) = (start.as_deref(), end.as_deref()) else {
        return 0;
    };
    let parse = |ts: &str| -> Option<i64> {
        if ts.len() < 19 {
            return None;
        }
        let y: i64 = ts[0..4].parse().ok()?;
        let mo: i64 = ts[5..7].parse().ok()?;
        let d: i64 = ts[8..10].parse().ok()?;
        let h: i64 = ts[11..13].parse().ok()?;
        let mi: i64 = ts[14..16].parse().ok()?;
        let se: i64 = ts[17..19].parse().ok()?;
        Some(((((y * 12 + mo) * 31 + d) * 24 + h) * 60 + mi) * 60 + se)
    };
    match (parse(s), parse(e)) {
        (Some(a), Some(b)) if b > a => ((b - a) as u64) * 1000,
        _ => 0,
    }
}

fn read_workspace_slug(ws_dir: &Path) -> String {
    let ws_json_path = ws_dir.join("workspace.json");
    let content = match std::fs::read_to_string(&ws_json_path) {
        Ok(c) => c,
        Err(_) => return "cursor".to_string(),
    };
    let parsed: WorkspaceJson = match serde_json::from_str(&content) {
        Ok(w) => w,
        Err(_) => return "cursor".to_string(),
    };
    parsed
        .folder
        .as_deref()
        .and_then(|uri| uri.strip_prefix("file:///").or(uri.strip_prefix("file://")))
        .and_then(|path| Path::new(path).file_name())
        .and_then(|name| name.to_str())
        .unwrap_or("cursor")
        .to_string()
}
