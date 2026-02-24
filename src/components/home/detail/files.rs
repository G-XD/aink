//! Files tab: grouped file tree with common prefix stripping.

use std::collections::BTreeMap;

use ratatui::prelude::*;
use ratatui::text::Line;

use crate::collector::transcript::TranscriptData;
use crate::components::common::theme;

/// Normalize backslashes to forward slashes for consistent display.
fn normalize_sep(path: &str) -> String {
    path.replace('\\', "/")
}

/// Find the longest common directory prefix among file paths.
/// Returns the prefix including the trailing `/`, or empty if none.
/// Expects paths already normalized to forward-slash separators.
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

/// Build file tree content lines for scrollable Paragraph.
pub fn detail_files_content(data: &TranscriptData, _width: u16) -> Vec<Line<'static>> {
    let mut lines: Vec<Line<'static>> = Vec::new();
    let file_count = data.files_touched.len();

    lines.push(Line::from(vec![
        Span::styled("  ", theme::label_style()),
        Span::styled(format!("{}", file_count), theme::stat_number_style()),
        Span::styled(" files touched", theme::label_style()),
    ]));
    lines.push(Line::from(Span::styled(
        format!("  {}", theme::SEP_LIGHT.repeat(40)),
        theme::separator_style(),
    )));
    lines.push(Line::from(""));

    if file_count == 0 {
        lines.push(Line::from(Span::styled(
            "  No files modified",
            theme::fold_style(),
        )));
        return lines;
    }

    let normalized: Vec<String> = data
        .files_touched
        .iter()
        .map(|p| normalize_sep(p))
        .collect();
    let common_prefix = common_dir_prefix(&normalized);
    let relative_paths: Vec<String> = normalized
        .iter()
        .map(|p| p[common_prefix.len()..].to_string())
        .collect();

    if !common_prefix.is_empty() {
        lines.push(Line::from(Span::styled(
            format!("  {}", common_prefix),
            theme::fold_style(),
        )));
        lines.push(Line::from(""));
    }

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
        let dir_connector = if is_last_dir { "\u{2514}" } else { "\u{251c}" };
        let child_prefix = if is_last_dir { "  " } else { "\u{2502} " };

        lines.push(Line::from(vec![
            Span::styled(format!("  {} ", dir_connector), theme::tree_style()),
            Span::styled(format!("{}/", dir), theme::section_title_style()),
            Span::styled(format!(" ({})", files.len()), theme::stat_secondary_style()),
        ]));
        for (i, file) in files.iter().enumerate() {
            let connector = if i == files.len() - 1 {
                "\u{2514} "
            } else {
                "\u{251c} "
            };
            lines.push(Line::from(vec![
                Span::styled(
                    format!("  {} {}", child_prefix, connector),
                    theme::tree_style(),
                ),
                Span::styled(file.clone(), theme::file_style(file)),
            ]));
        }
    }

    if !root_files.is_empty() {
        for (i, file) in root_files.iter().enumerate() {
            let connector = if i == root_files.len() - 1 {
                "\u{2514} "
            } else {
                "\u{251c} "
            };
            lines.push(Line::from(vec![
                Span::styled(format!("  {}", connector), theme::tree_style()),
                Span::styled(file.clone(), theme::file_style(file)),
            ]));
        }
    }

    lines
}
