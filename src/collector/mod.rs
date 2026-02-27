pub mod claude;
pub mod codex;
pub mod cursor;
pub mod source;
pub mod transcript;

use std::path::{Path, PathBuf};
use tracing::warn;

/// Recursively collect all `.jsonl` files under `dir`, skipping directories
/// whose name appears in `skip_dirs`.
pub fn collect_jsonl_paths(dir: &Path, out: &mut Vec<PathBuf>, skip_dirs: &[&str]) {
    let read_dir = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) => {
            warn!("read_dir {}: {}", dir.display(), e);
            return;
        }
    };
    for entry in read_dir.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| skip_dirs.contains(&n))
            {
                continue;
            }
            collect_jsonl_paths(&path, out, skip_dirs);
        } else if path.extension().is_some_and(|e| e == "jsonl") {
            out.push(path);
        }
    }
}
