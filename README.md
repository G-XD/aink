# aink

**Track and analyze AI coding tool usage.**

[中文](README.zh-CN.md)

AINK is a powerful terminal UI (TUI) application that discovers, loads, and analyzes AI coding session transcripts from multiple sources. Track your usage of Claude Code, Cursor, Codex, Kiro, and other AI coding tools—monitor tokens, costs, tool calls, edited files, and full conversation history—all in one unified interface.

Built with Rust and ratatui, AINK provides a fast, keyboard-driven experience for developers who want to understand and optimize their AI coding tool usage.

---

## Quick Start

### Install

#### Homebrew

```bash
brew install g-xd/tap/aink
```

### Run

```bash
aink
```

AINK will automatically discover sessions from supported AI coding tools.

---

## Features

### 🔍 Multi-source transcript discovery

AINK automatically discovers and aggregates sessions from multiple AI coding tools:

- **Claude Code** — sessions stored in `~/.claude/projects` (native token counts from API)
- **Cursor IDE** — Cursor project transcripts from application data (tokens calculated via tiktoken)
- **OpenAI Codex CLI** — Codex session data
- **Kiro** — sessions stored in `~/.kiro/sessions` (tokens calculated via tiktoken)

Each source can be individually enabled or disabled in `config.json5`. AINK intelligently merges sessions from all enabled sources into a unified, sortable view, making it easy to compare usage across different tools.

**Token Calculation**: Cursor and Kiro sessions don't include native token counts in their data. AINK uses the [tiktoken-rs](https://github.com/zurawiki/tiktoken-rs) library (cl100k_base encoding) to accurately calculate token counts from conversation text. For bulk loading performance, Kiro uses a fast character-based estimation with precise tiktoken counting available in detail view.

### 📊 Session list and sorting

View all your AI coding sessions in a comprehensive table:

- **Rich metadata**: Project name, model, date, duration, token counts, and estimated cost
- **Flexible sorting**: Sort by date, tokens, cost, or duration to quickly identify high-usage sessions
- **Expandable rows**: Press `→` to expand any session and see per-model token breakdown
- **Live refresh**: Press `r` to reload sessions from disk without restarting
- **Time filtering**: Filter sessions by date range (today, last 7 days, last 30 days, or custom range)
- **CLI filtering**: Start with `--range` argument to immediately filter sessions

The zebra-striped table design with selection highlighting makes it easy to scan through hundreds of sessions.

### 🔎 Session detail view

Open any session to explore comprehensive details across three organized tabs:

**Stats Tab**
- Session metadata: duration, start time, turn count, cost, git branch, version
- Token breakdown: input, output, cache write, cache read, and hit rate
- Model usage: visual bars showing token distribution across models
- Tool call distribution: frequency and percentage breakdown by tool type

**Conversation Tab**
- Full turn-by-turn conversation history
- User and assistant messages with clear role indicators
- Tool call summaries with expandable details
- Foldable sections to manage long conversations

**Files Tab**
- Complete list of files touched during the session
- Grouped by directory for easy navigation
- Tree-style visualization with connectors

Navigate between tabs using `Tab`/`Shift+Tab` or direct shortcuts (`S`, `C`, `F`).

### 📤 Export sessions

Export any session to Markdown format for sharing, documentation, or archiving:

- **One-key export**: Press `e` in list view or detail view to export the current session
- **Comprehensive content**: Exported files include:
  - Session metadata (source, date, duration, branch, version)
  - Summary and statistics
  - Model usage breakdown with token counts
  - Full conversation history with tool calls
  - Complete list of files touched
  - Tool call summary and distribution
- **Smart location**: Export files are saved to the most convenient location:
  1. `./aink-exports/` — current working directory (most discoverable)
  2. `~/Documents/aink-exports/` — fallback if current directory is not writable
  3. System data directory — last resort
- **Timestamped filenames**: Format `aink-export-{project-name}-{timestamp}.md` prevents overwrites

The export path is displayed in the success message, making it easy to locate your exported files.

### 💰 Cost and usage analytics

Get instant insights into your AI coding tool spending and usage patterns:

- **Per-session costs**: Accurate cost estimates based on published model pricing
- **Token tracking**: Separate input and output token counts for precise analysis
  - Claude Code: Native token counts from API responses
  - Cursor & Kiro: Calculated using tiktoken-rs (cl100k_base encoding)
- **Cache efficiency**: Monitor cache hit rates and cache token usage to optimize costs
- **Model comparison**: See which models you use most and their relative costs
- **Tool activity**: Track tool call frequency and file edit counts
- **Aggregate views**: Overview and Analysis tabs provide project-level and tool-level summaries

Cost estimates are configurable via `config.json5` to match your pricing tier or custom rates. Since Cursor and Kiro sessions lack native token data, costs are calculated based on tiktoken-derived token counts.

### ⌨️ Keyboard-driven TUI

Designed for developers who prefer keyboard navigation:

- **Three main tabs**: Overview, Sessions, Analysis
- **Vim-style navigation**: `j`/`k` for up/down, `h`/`l` for expand/collapse
- **Quick tab switching**: Number keys (`1`, `2`, `3`) or `Tab`/`Shift+Tab`
- **Contextual shortcuts**: Different key bindings for list vs detail view
- **Responsive layout**: Adapts to terminal size with wide and narrow layouts
- **Cyberpunk theme**: Dark theme with accent colors, zebra stripes, and clean separators
- **Configurable**: Customize keybindings and styles via `config.json5`

The interface is optimized for speed—no mouse required, no unnecessary animations, just fast data access.

---

## Architecture

AINK is built with a modular architecture:

- **Collector layer**: Pluggable transcript sources (Claude, Cursor, Codex, Kiro) with unified parsing
- **Data model**: Normalized `TranscriptData` structure with per-model statistics and conversation history
- **Component system**: Reusable UI components (tabs, tables, detail views) with independent state
- **Action-based updates**: Event-driven architecture using tokio channels for async operations
- **Theme system**: Centralized styling with automatic dark/light mode detection

The codebase follows Rust best practices with comprehensive tests, including property-based testing for critical functionality like export filename generation.

---

## Configuration

Config is loaded from the platform config directory and merged with built-in defaults:

- **macOS:** `~/Library/Application Support/aink/`
- **Linux:** `~/.config/aink/`
- **Windows:** `%APPDATA%/aink/`

Override locations:

```bash
AINK_CONFIG=/path/to/config AINK_DATA=/path/to/data aink
```

### Example config

Example `config.json5` with multiple sources:

```json5
{
  "sources": [
    { "kind": "Claude", "root_dir": "~/.claude/projects", "enabled": true },
    { "kind": "Cursor", "root_dir": "~/Library/Application Support/Cursor/User", "enabled": true },
    { "kind": "Codex", "root_dir": "~/.codex/sessions", "enabled": true },
    { "kind": "Kiro", "root_dir": "~/.kiro/sessions", "enabled": true }
  ]
}
```

Keybindings and styles can be customized in `config.json5` (see defaults in the config directory after first run).

### Environment variables

| Variable        | Description                    |
|----------------|--------------------------------|
| `AINK_CONFIG`  | Override config directory      |
| `AINK_DATA`   | Override data directory        |
| `AINK_LOG_LEVEL` | Log level (default: `INFO`) |
| `RUST_LOG`    | Alternative log level control   |

Logs are written to the data directory (e.g. `~/.local/share/aink/aink.log` on Linux).

---

## Build & Develop

```bash
cargo build              # Debug
cargo build --release    # Release (optimized)
cargo test               # Tests
cargo clippy             # Lint
cargo run                # Run
```

---

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
