//! Theme system: auto-detects light/dark terminal and selects matching palette.
//! All colors centralized via `palette()` for easy swapping.

use std::sync::OnceLock;

use ratatui::style::{Color, Modifier, Style};

// ── Theme detection ──────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum ThemeMode {
    Dark,
    Light,
}

struct Palette {
    cyan: Color,
    teal: Color,
    magenta: Color,
    amber: Color,
    green: Color,
    red: Color,
    dim: Color,
    muted: Color,
    normal: Color,
    bright: Color,
    highlight_bg: Color,
    zebra_bg: Color,
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
            cyan: Color::Rgb(0, 255, 255),
            teal: Color::Rgb(80, 200, 200),
            magenta: Color::Rgb(255, 0, 255),
            amber: Color::Rgb(255, 176, 0),
            green: Color::Rgb(0, 255, 136),
            red: Color::Rgb(255, 51, 102),
            dim: Color::Rgb(88, 96, 110),
            muted: Color::Rgb(138, 148, 164),
            normal: Color::Rgb(180, 185, 195),
            bright: Color::Rgb(230, 235, 245),
            highlight_bg: Color::Rgb(25, 30, 45),
            zebra_bg: Color::Rgb(18, 20, 28),
        },
        ThemeMode::Light => Palette {
            cyan: Color::Rgb(0, 130, 155),
            teal: Color::Rgb(0, 110, 120),
            magenta: Color::Rgb(160, 0, 140),
            amber: Color::Rgb(180, 110, 0),
            green: Color::Rgb(0, 140, 60),
            red: Color::Rgb(200, 30, 60),
            dim: Color::Rgb(158, 164, 176),
            muted: Color::Rgb(104, 110, 124),
            normal: Color::Rgb(50, 55, 65),
            bright: Color::Rgb(20, 25, 35),
            highlight_bg: Color::Rgb(215, 220, 232),
            zebra_bg: Color::Rgb(235, 238, 245),
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
        .fg(palette().teal)
        .add_modifier(Modifier::BOLD)
}

/// Table header row.
pub fn header_style() -> Style {
    Style::default()
        .fg(palette().teal)
        .add_modifier(Modifier::BOLD)
}

/// Normal body text in tables/lists.
pub fn body_style() -> Style {
    Style::default().fg(palette().normal)
}

/// Selected row: entire row background + bright text.
pub fn highlight_style() -> Style {
    Style::default()
        .bg(palette().highlight_bg)
        .fg(palette().bright)
        .add_modifier(Modifier::BOLD)
}

/// Selection indicator bar (left │).
pub fn selection_bar_style() -> Style {
    Style::default().fg(palette().cyan)
}

/// Zebra stripe background for odd rows.
pub fn zebra_style() -> Style {
    Style::default().bg(palette().zebra_bg).fg(palette().normal)
}

/// Footer hints: description text.
pub fn footer_style() -> Style {
    Style::default().fg(palette().dim)
}

/// Footer hints: key name (highlighted).
pub fn footer_key_style() -> Style {
    Style::default().fg(palette().cyan)
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
