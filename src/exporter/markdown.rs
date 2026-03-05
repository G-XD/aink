use crate::collector::transcript::{ConversationRole, ConversationTurn, TranscriptData};
use crate::utils::format::{format_cost, format_duration, format_tokens, parse_local_datetime};
use color_eyre::Result;
use std::path::Path;

/// Generate complete Markdown document
pub fn generate_markdown(
    _session_path: &Path,
    data: &TranscriptData,
    conversation: Option<&[ConversationTurn]>,
) -> Result<String> {
    let mut md = String::new();

    // Title
    let project_name = data
        .project_name
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or("Unknown Project");
    md.push_str(&format!("# AI Coding Session: {}\n\n", project_name));
    md.push_str(&format!("# AI Coding Session: {}\n\n", project_name));

    // Metadata
    md.push_str(&format_metadata(data));
    md.push_str("\n\n");

    // Summary
    md.push_str("## Summary\n\n");
    md.push_str(
        data.summary
            .as_deref()
            .or(data.first_user_message.as_deref())
            .unwrap_or("(no summary available)"),
    );
    md.push_str("\n\n");

    // Statistics
    md.push_str(&format_statistics(data));
    md.push_str("\n\n");

    // Models
    md.push_str(&format_models(data));
    md.push_str("\n\n");

    // Conversation
    md.push_str(&format_conversation(conversation));
    md.push_str("\n\n");

    // Files
    md.push_str(&format_files(data));
    md.push_str("\n\n");

    // Tool calls
    md.push_str(&format_tool_calls(data));
    md.push_str("\n\n");

    // Timestamp
    md.push_str("---\n\n");
    md.push_str(&format!(
        "*Exported: {} By AINK*",
        chrono::Local::now().format("%Y-%m-%d %H:%M")
    ));

    Ok(md)
}

fn format_metadata(data: &TranscriptData) -> String {
    let started = data
        .start_time
        .as_deref()
        .and_then(|ts| parse_local_datetime(ts))
        .map(|(y, mo, d, h, mi)| format!("{:04}-{:02}-{:02} {:02}:{:02}", y, mo, d, h, mi))
        .unwrap_or_else(|| "—".to_string());

    format!(
        "**Source:** {}\n\n**Started:** {}\n\n**Duration:** {}\n\n**Branch:** {}\n\n**Version:** {}",
        data.source,
        started,
        format_duration(data.duration_ms),
        data.git_branch.as_deref().unwrap_or("—"),
        data.agent_version.as_deref().unwrap_or("—")
    )
}

fn format_statistics(data: &TranscriptData) -> String {
    let total_tokens = data.input_tokens
        + data.output_tokens
        + data.cache_creation_tokens
        + data.cache_read_tokens;
    let cache_hit_rate = if data.cache_creation_tokens + data.cache_read_tokens > 0 {
        let total_cache = data.cache_creation_tokens + data.cache_read_tokens;
        (data.cache_read_tokens as f64 / total_cache as f64 * 100.0).round() as u32
    } else {
        0
    };

    format!(
        "## Statistics\n\n\
        | Metric | Value |\n\
        |--------|-------|\n\
        | Total Tokens | {} |\n\
        | Input Tokens | {} |\n\
        | Output Tokens | {} |\n\
        | Cache Write | {} |\n\
        | Cache Read | {} |\n\
        | Cache Hit Rate | {}% |\n\
        | Estimated Cost | {} |\n\
        | Turns | {} |\n\
        | Messages | {}u / {}a |",
        format_tokens(total_tokens),
        format_tokens(data.input_tokens),
        format_tokens(data.output_tokens),
        format_tokens(data.cache_creation_tokens),
        format_tokens(data.cache_read_tokens),
        cache_hit_rate,
        format_cost(data.estimated_cost_usd),
        data.turn_count,
        data.user_message_count,
        data.assistant_message_count
    )
}

fn format_models(data: &TranscriptData) -> String {
    let mut md = String::from("## Models Used\n\n");

    if data.per_model.is_empty() {
        md.push_str("(no model information available)");
        return md;
    }

    let mut models: Vec<_> = data.per_model.iter().collect();
    models.sort_by_key(|(name, _)| *name);

    for (model_name, stats) in models {
        md.push_str(&format!(
            "- **{}**: {} in / {} out\n",
            model_name,
            format_tokens(stats.input_tokens),
            format_tokens(stats.output_tokens)
        ));
    }

    md
}

fn format_conversation(conversation: Option<&[ConversationTurn]>) -> String {
    let mut md = String::from("## Conversation\n\n");

    let Some(turns) = conversation else {
        md.push_str("(conversation data not available)");
        return md;
    };

    if turns.is_empty() {
        md.push_str("(conversation data not available)");
        return md;
    }

    let total_turns = turns.len();

    for (idx, turn) in turns.iter().enumerate() {
        let role_str = match turn.role {
            ConversationRole::User => "User",
            ConversationRole::Assistant => "Assistant",
        };

        let time_str = turn
            .created_at
            .as_deref()
            .and_then(|ts| parse_local_datetime(ts))
            .map(|(_, _, _, h, mi)| format!("{:02}:{:02}", h, mi))
            .unwrap_or_else(|| "".to_string());

        md.push_str(&format!(
            "### Turn {}/{} — {} ({})\n\n",
            idx + 1,
            total_turns,
            role_str,
            time_str
        ));

        if !turn.tool_calls.is_empty() {
            md.push_str("**Tool Calls:**\n");
            for tool_call in &turn.tool_calls {
                if tool_call.summary.is_empty() {
                    md.push_str(&format!("- `{}`\n", tool_call.name));
                } else {
                    md.push_str(&format!("- `{}` — {}\n", tool_call.name, tool_call.summary));
                }
            }
            md.push('\n');
        }

        md.push_str(&turn.content);
        md.push_str("\n\n");
    }

    md
}

fn format_files(data: &TranscriptData) -> String {
    use std::collections::BTreeMap;

    let file_count = data.files_touched.len();
    let mut md = format!("## Files Touched ({} files)\n\n", file_count);

    if file_count == 0 {
        md.push_str("(no files touched)");
        return md;
    }

    let normalized: Vec<String> = data
        .files_touched
        .iter()
        .map(|p| p.replace('\\', "/"))
        .collect();
    let common_prefix = common_dir_prefix(&normalized);
    let relative_paths: Vec<String> = normalized
        .iter()
        .map(|p| p[common_prefix.len()..].to_string())
        .collect();

    md.push_str("```\n");

    let mut dirs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut root_files: Vec<String> = Vec::new();

    for rel_path in &relative_paths {
        if let Some(slash_pos) = rel_path.rfind('/') {
            let dir = rel_path[..slash_pos].to_string();
            let name = rel_path[slash_pos + 1..].to_string();
            dirs.entry(dir).or_default().push(name);
        } else {
            root_files.push(rel_path.clone());
        }
    }

    let dir_count = dirs.len() + if root_files.is_empty() { 0 } else { 1 };
    let mut dir_idx = 0;

    for (dir, files) in &dirs {
        dir_idx += 1;
        let is_last_dir = dir_idx == dir_count;
        let dir_connector = if is_last_dir { "└" } else { "├" };
        let child_prefix = if is_last_dir { "  " } else { "│ " };

        md.push_str(&format!("{} {}/\n", dir_connector, dir));

        for (i, file) in files.iter().enumerate() {
            let connector = if i == files.len() - 1 { "└" } else { "├" };
            md.push_str(&format!("{}  {} {}\n", child_prefix, connector, file));
        }
    }

    if !root_files.is_empty() {
        for (i, file) in root_files.iter().enumerate() {
            let connector = if i == root_files.len() - 1 {
                "└"
            } else {
                "├"
            };
            md.push_str(&format!("{} {}\n", connector, file));
        }
    }

    md.push_str("```");
    md
}

fn common_dir_prefix(paths: &[String]) -> String {
    if paths.is_empty() {
        return String::new();
    }
    let first = &paths[0];
    let mut prefix_end = 0;
    for (i, ch) in first.char_indices() {
        if paths[1..].iter().any(|p| !p[..].starts_with(&first[..=i])) {
            break;
        }
        if ch == '/' {
            prefix_end = i + 1;
        }
    }
    first[..prefix_end].to_string()
}

fn format_tool_calls(data: &TranscriptData) -> String {
    let mut md = String::from("## Tool Call Summary\n\n");

    if data.tool_call_by_type.is_empty() {
        md.push_str("(no tool calls)");
        return md;
    }

    md.push_str("| Tool | Count | Percentage |\n");
    md.push_str("|------|-------|------------|\n");

    let mut tool_calls: Vec<_> = data.tool_call_by_type.iter().collect();
    tool_calls.sort_by(|a, b| b.1.cmp(a.1));

    let total_calls = data.tool_call_total;

    for (tool_name, count) in &tool_calls {
        let percentage = if total_calls > 0 {
            ((**count as f64 / total_calls as f64) * 100.0).round() as u32
        } else {
            0
        };
        md.push_str(&format!(
            "| {} | {} | {}% |\n",
            tool_name, count, percentage
        ));
    }

    let tool_type_count = data.tool_call_by_type.len();
    md.push_str(&format!(
        "\n**Total:** {} tool calls across {} types",
        total_calls, tool_type_count
    ));

    md
}
