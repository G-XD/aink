//! Theme system: auto-detects light/dark terminal and selects matching palette.
//! All colors centralized via `palette()` for easy swapping.

use std::sync::OnceLock;

use ratatui::style::{Color, Modifier, Style};

use crate::collector::source::SourceKind;

// ── Theme detection ──────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum ThemeMode {
    Dark,
    Light,
}

struct Palette {
    cyan: Color,
    magenta: Color,
    amber: Color,
    green: Color,
    red: Color,
    dim: Color,
    muted: Color,
    normal: Color,
    bright: Color,
    highlight_bg: Color,
}

static PALETTE: OnceLock<Palette> = OnceLock::new();

fn detect_theme() -> ThemeMode {
    if let Ok(val) = std::env::var("COLORFGBG")
        && let Some(bg) = val.rsplit(';').next()
        && let Ok(n) = bg.parse::<u8>()
    {
        // ANSI colors 0-6 are dark backgrounds, 7+ are light
        return if n >= 7 {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        };
    }
    ThemeMode::Dark
}

fn palette() -> &'static Palette {
    PALETTE.get_or_init(|| match detect_theme() {
        ThemeMode::Dark => Palette {
            cyan: Color::Rgb(100, 200, 255),
            magenta: Color::Rgb(200, 120, 255),
            amber: Color::Rgb(255, 180, 50),
            green: Color::Rgb(80, 250, 150),
            red: Color::Rgb(255, 85, 110),
            dim: Color::Rgb(55, 60, 85),
            muted: Color::Rgb(130, 140, 175),
            normal: Color::Rgb(200, 208, 225),
            bright: Color::Rgb(240, 242, 250),
            highlight_bg: Color::Rgb(55, 60, 130),
        },
        ThemeMode::Light => Palette {
            cyan: Color::Rgb(0, 120, 190),
            magenta: Color::Rgb(140, 50, 200),
            amber: Color::Rgb(190, 100, 0),
            green: Color::Rgb(0, 135, 60),
            red: Color::Rgb(210, 40, 60),
            dim: Color::Rgb(170, 175, 188),
            muted: Color::Rgb(90, 96, 115),
            normal: Color::Rgb(40, 45, 55),
            bright: Color::Rgb(15, 18, 28),
            highlight_bg: Color::Rgb(200, 210, 240),
        },
    })
}

// ── Separator characters ─────────────────────────────────────
pub const SEP_HEAVY: &str = "━";
pub const SEP_LIGHT: &str = "─";
pub const SEP_DASH: &str = "╌";

// ── Style functions ──────────────────────────────────────────

/// Tab bar: active tab label.
pub fn tab_active_style() -> Style {
    Style::default()
        .fg(palette().cyan)
        .add_modifier(Modifier::BOLD)
}

/// Tab bar: inactive tab label.
pub fn tab_inactive_style() -> Style {
    Style::default().fg(palette().muted)
}

/// Tab bar: key hint (the "1", "2", "3" numbers).
pub fn tab_key_style() -> Style {
    Style::default().fg(palette().dim)
}

/// Primary separator line.
pub fn separator_style() -> Style {
    Style::default().fg(palette().dim)
}

/// Section title (e.g. "Tokens", "Tool Calls").
pub fn section_title_style() -> Style {
    Style::default()
        .fg(palette().cyan)
        .add_modifier(Modifier::BOLD)
}

/// Table header row.
pub fn header_style() -> Style {
    Style::default()
        .fg(palette().cyan)
        .add_modifier(Modifier::BOLD)
}

/// Normal body text in tables/lists.
pub fn body_style() -> Style {
    Style::default().fg(palette().normal)
}

/// Selected row: entire row background + cyan text.
pub fn highlight_style() -> Style {
    Style::default()
        .bg(palette().highlight_bg)
        .fg(palette().cyan)
        .add_modifier(Modifier::BOLD)
}

/// Selection indicator bar (left │).
pub fn selection_bar_style() -> Style {
    Style::default()
        .fg(palette().amber)
        .add_modifier(Modifier::BOLD)
}

/// Footer hints: description text.
pub fn footer_style() -> Style {
    Style::default().fg(palette().dim)
}

/// Footer hints: key name (highlighted).
pub fn footer_key_style() -> Style {
    Style::default()
        .fg(palette().amber)
        .add_modifier(Modifier::BOLD)
}

/// Empty state message.
pub fn empty_msg_style() -> Style {
    Style::default().fg(palette().muted)
}

/// Detail view: field label (e.g. "Input tokens").
pub fn label_style() -> Style {
    Style::default().fg(palette().muted)
}

/// Detail view: field value.
pub fn value_style() -> Style {
    Style::default().fg(palette().bright)
}

/// Stat numbers (large/prominent).
pub fn stat_number_style() -> Style {
    Style::default()
        .fg(palette().amber)
        .add_modifier(Modifier::BOLD)
}

/// Secondary/dimmed stat text.
pub fn stat_secondary_style() -> Style {
    Style::default().fg(palette().muted)
}

/// Model name display.
pub fn model_style() -> Style {
    Style::default().fg(palette().magenta)
}

/// Bar chart: filled portion.
pub fn bar_filled_style() -> Style {
    Style::default().fg(palette().cyan)
}

/// Bar chart: empty portion.
pub fn bar_empty_style() -> Style {
    Style::default().fg(palette().dim)
}

/// Braille chart style (for cache hit rate, etc.).
pub fn braille_style() -> Style {
    Style::default().fg(palette().magenta)
}

/// Conversation: User label.
pub fn conv_user_style() -> Style {
    Style::default()
        .fg(palette().magenta)
        .add_modifier(Modifier::BOLD)
}

/// Conversation: Assistant label.
pub fn conv_assistant_style() -> Style {
    Style::default()
        .fg(palette().green)
        .add_modifier(Modifier::BOLD)
}

/// Collapsible fold indicator (▸/▾ and summary text).
pub fn fold_style() -> Style {
    Style::default().fg(palette().dim)
}

/// Tree connector characters (├ └ │).
pub fn tree_style() -> Style {
    Style::default().fg(palette().dim)
}

/// Bar chart: filled portion in green (for tool distribution).
pub fn bar_filled_style_green() -> Style {
    Style::default().fg(palette().green)
}

/// Source kind badge: distinct color per AI tool.
pub fn source_style(kind: SourceKind) -> Style {
    let color = match kind {
        SourceKind::Claude => Color::Rgb(230, 140, 60), // warm orange (Claude brand)
        SourceKind::Cursor => Color::Rgb(90, 140, 255), // blue (Cursor brand)
        SourceKind::Codex => Color::Rgb(50, 210, 130),  // green (OpenAI/Codex)
        SourceKind::Kiro => Color::Rgb(150, 100, 255),  // purple (Kiro brand)
    };
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

/// File name colored by extension category.
pub fn file_style(filename: &str) -> Style {
    let ext = filename.rsplit('.').next().unwrap_or("");
    let color = match ext {
        "rs" | "go" | "py" | "js" | "ts" | "tsx" | "jsx" | "c" | "cpp" | "h" | "java" | "rb"
        | "swift" | "kt" | "zig" | "lua" | "sh" | "zsh" | "bash" | "el" | "ex" | "exs" | "hs"
        | "ml" | "pl" | "r" | "scala" | "vue" => palette().cyan,
        "toml" | "yaml" | "yml" | "json" | "json5" | "ini" | "env" | "lock" | "xml" | "plist"
        | "conf" | "cfg" => palette().amber,
        "md" | "txt" | "rst" | "adoc" | "doc" | "org" | "tex" | "rtf" => palette().green,
        "css" | "scss" | "less" | "html" | "svg" | "sass" | "styl" => palette().magenta,
        _ => palette().bright,
    };
    Style::default().fg(color)
}

/// Cost value colored by magnitude: green (low), amber (mid), red (high).
pub fn cost_style(usd: f64) -> Style {
    let color = if usd < 0.50 {
        palette().green
    } else if usd < 2.0 {
        palette().amber
    } else {
        palette().red
    };
    Style::default().fg(color)
}
