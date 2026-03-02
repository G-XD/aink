# aink

**Track and analyze AI coding tool usage.**

[中文](README.zh-CN.md)

AINK is a terminal UI (TUI) that discovers, loads, and analyzes AI coding session transcripts. Use it to see how much you use Claude, Cursor, Codex, and other tools—tokens, cost, tool calls, edited files, and full conversation history—all in one place.

---

## Features

### Multi-source transcript discovery

- **Claude Code** — sessions under `~/.claude/projects` (or your configured root)
- **Cursor IDE** — Cursor project transcripts
- **OpenAI Codex CLI** — Codex session data

Configure one or more sources in `config.json5`; AINK merges sessions from all enabled sources into a single list. You can add custom roots and enable/disable sources per environment.

### Session list and sorting

- Table view of all sessions: project name, model, date, duration, token counts, and estimated cost
- Sort by date, tokens, cost, or duration to find heavy or recent sessions quickly
- Refresh from disk on demand to pick up new transcripts

### Session detail view

For any session you can open a detail view with:

- **Stats** — input/output token breakdown, model name, duration, and cost estimate
- **Conversation** — full turn-by-turn history (user messages and assistant replies)
- **Files** — list of files touched during the session (created, read, or edited)

Use sub-tabs and keybindings to move between Stats, Conversation, and Files.

### Cost and usage at a glance

- Per-session and aggregate token usage (input vs output)
- Cost estimates based on published model pricing (configurable)
- Tool-call and file-edit counts to see how “heavy” a session was

### TUI experience

- Keyboard-driven navigation and tab switching (Overview, Sessions, Analysis)
- Configurable keybindings and styles via `config.json5`

---

## Install & run

**From release:** download the latest [Release](https://github.com/YOUR_USERNAME/aink/releases) for your platform, unpack, and add `aink` to your `PATH`.

**From source** (requires [Rust](https://rustup.rs) 1.70+):

```bash
git clone https://github.com/YOUR_USERNAME/aink.git && cd aink && cargo install --path .
```

Then run:

```bash
aink
```

Flags: `aink --help`, `aink --version`.

## Configuration

Config is loaded from the platform config directory and merged with built-in defaults:

- **macOS:** `~/Library/Application Support/aink/`
- **Linux:** `~/.config/aink/`
- **Windows:** `%APPDATA%/aink/`

Override locations:

```bash
AINK_CONFIG=/path/to/config AINK_DATA=/path/to/data aink
```

Example config (JSON5) with multiple sources:

```json5
{
  "sources": [
    { "kind": "Claude", "root_dir": "~/.claude/projects", "enabled": true },
    { "kind": "Cursor", "root_dir": "~/Library/Application Support/Cursor/User", "enabled": true },
    { "kind": "Codex", "root_dir": "~/.codex/sessions", "enabled": true }
  ]
}
```

Keybindings and styles can be customized in `config.json5` (see defaults in the config directory after first run).

## Environment variables

| Variable        | Description                    |
|----------------|--------------------------------|
| `AINK_CONFIG`  | Override config directory      |
| `AINK_DATA`   | Override data directory        |
| `AINK_LOG_LEVEL` | Log level (default: `INFO`) |
| `RUST_LOG`    | Alternative log level control   |

Logs are written to the data directory (e.g. `~/.local/share/aink/aink.log` on Linux).

## Build & develop

```bash
cargo build              # Debug
cargo build --release    # Release (optimized)
cargo test               # Tests
cargo clippy             # Lint
cargo run                # Run
```

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
