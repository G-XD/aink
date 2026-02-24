# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

AINK is a Rust TUI application that discovers, loads, and analyzes AI coding session transcripts. It provides an interactive terminal interface to browse sessions with statistics on token usage, model usage, tool calls, edited files, and full conversation history. Currently supports Claude Code transcripts; designed to be extensible to Codex, Cursor, and other AI coding tools.

## Build & Development Commands

```bash
cargo build              # Debug build
cargo build --release    # Optimized release build (LTO, size-optimized, stripped)
cargo test               # Run all tests
cargo test <test_name>   # Run a single test by name
cargo clippy             # Lint checking
cargo run                # Run the application
```

## Environment Variables

- `AINK_CONFIG` — Override config directory path
- `AINK_DATA` — Override data directory path
- `AINK_LOG_LEVEL` — Set log level (default: INFO)
- `RUST_LOG` — Alternative log level control

Logs write to `~/.local/share/aink/aink.log` (or platform-specific data directory).

## Architecture

### Event-Driven Component System

The app follows a unidirectional data flow pattern:

**User Input → Events → Actions → Component Updates → Render**

- `app.rs` — Main event loop. Polls crossterm events, dispatches `Action` variants through tokio MPSC channels to registered components. Manages tick/frame intervals and graceful shutdown via `CancellationToken`.
- `action.rs` — Central `Action` enum defining all user and system events (Quit, Render, Tick, RefreshTranscripts, etc.).
- `components.rs` — `Component` trait with lifecycle hooks: `init`, `handle_events`, `handle_key_event`, `update`, `draw`. All UI elements implement this trait.
- `tui.rs` — Abstraction over ratatui/crossterm for terminal setup, teardown, and drawing.

### Collector: Multi-Source Transcript System

The collector layer uses a **trait-based** architecture for extensibility:

- **`collector/source.rs`** — Defines `TranscriptSource` trait and `SourceKind` enum. Each AI tool implements this trait for discovery, parsing, and cost estimation. Factory function `create_source()` builds implementations from config.
- **`collector/transcript.rs`** — Source-agnostic data models (`TranscriptData`, `ConversationTurn`, `SessionList` type alias). Shared across all sources.
- **`collector/claude.rs`** — Claude Code implementation: JSONL parsing, subagent merging, Claude-specific cost estimation. Implements `TranscriptSource`.

To add a new source (e.g. Codex):
1. Add variant to `SourceKind` enum in `source.rs`
2. Create `collector/codex.rs` implementing `TranscriptSource`
3. Register in `create_source()` factory
4. Add default config in `Config::new()`

### Key Modules

- **`components/home/`** — Main UI screen with two view modes:
  - `table.rs` — List view: sortable table of all transcript sessions
  - `detail/` — Detail view: Stats, Conversation, Files sub-tabs
  - `view.rs` — View state enum (`List` vs `Detail(DetailState)`) with standalone `DetailState` struct
- **`components/tab_host.rs`** — Top-level tab container (Overview, Sessions, Analysis). Shares transcript data to child tabs via `Arc<SessionList>`.
- **`components/common/theme.rs`** — Centralized color and style definitions (RGB values for borders, headers, highlights, etc.)
- **`config.rs`** — Hierarchical config loading supporting JSON5, JSON, YAML, TOML, INI. Includes `SourceConfig` for multi-source support and a custom recursive-descent keybinding parser.
- **`utils/format.rs`** — Shared formatting helpers (`format_tokens`, `format_duration`, `format_cost`).
- **`utils/project_name.rs`** — Decodes Claude Code session paths (e.g., `-home-user-projects-myproject` → `myproject`) by stripping known prefixes and common directory names.
- **`errors.rs`** — Panic hooks that restore terminal state before printing, using `color-eyre` + `better-panic` + `human-panic`.
- **`build.rs`** — Embeds build date and git info via `vergen-gix`.

### Configuration

Default keybindings live in `.config/config.json5`. User config files are loaded from the platform-specific config directory (via the `directories` crate) and merged with defaults.

Multi-source configuration supports both legacy `transcript_dir` (auto-creates a Claude source) and explicit `sources` array:
```json5
{
  "sources": [
    { "kind": "Claude", "root_dir": "~/.claude/projects", "enabled": true }
  ]
}
```

### Testing

Tests use standard `#[test]` with `pretty_assertions`. Key test areas: config parsing (styles, keybinding sequences, round-trip serialization) and path decoding in `project_name.rs`.
