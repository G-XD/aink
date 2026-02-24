//! Footer hint bar: renders key/description pairs with highlighted keys.

use ratatui::text::{Line, Span};

use super::theme;

/// Build a footer line from `(key, description)` pairs.
/// Keys are rendered in `footer_key_style`, descriptions in `footer_style`.
pub fn footer_hints(pairs: &[(&str, &str)]) -> Line<'static> {
    let mut spans: Vec<Span> = vec![Span::raw(" ")];
    for (i, (key, desc)) in pairs.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  \u{2502}  ", theme::footer_style()));
        }
        spans.push(Span::styled(key.to_string(), theme::footer_key_style()));
        spans.push(Span::styled(format!(" {}", desc), theme::footer_style()));
    }
    Line::from(spans)
}
