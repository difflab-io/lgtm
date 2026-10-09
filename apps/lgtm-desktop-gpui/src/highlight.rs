//! syntect-powered syntax highlighting. Builds the default theme + syntax
//! set once at startup, then classifies one line at a time into
//! `Vec<(fg_rgb, text)>` spans the review view can render as colored children.

use std::sync::OnceLock;

use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};
use syntect::util::LinesWithEndings;

pub struct Highlighter {
    syntaxes: SyntaxSet,
    theme: syntect::highlighting::Theme,
}

static INSTANCE: OnceLock<Highlighter> = OnceLock::new();

impl Highlighter {
    pub fn global() -> &'static Self {
        INSTANCE.get_or_init(Self::new)
    }

    fn new() -> Self {
        let syntaxes = SyntaxSet::load_defaults_newlines();
        let theme = ThemeSet::load_defaults().themes["base16-eighties.dark"].clone();
        Self { syntaxes, theme }
    }

    /// Return syntax highlighting spans (fg rgb hex, text) for one line.
    /// `None` → unknown language; caller should fall back to plain text.
    pub fn highlight_line(&self, lang: Option<&str>, text: &str) -> Option<Vec<(u32, String)>> {
        let syntax = self.resolve_syntax(lang?)?;
        let mut h = syntect::easy::HighlightLines::new(syntax, &self.theme);
        let line = LinesWithEndings::from(text).next().unwrap_or(text);
        let regions = h.highlight_line(line, &self.syntaxes).ok()?;
        Some(regions.into_iter().map(style_to_rgb).collect())
    }

    fn resolve_syntax(&self, lang: &str) -> Option<&SyntaxReference> {
        // Try several lookup shapes so callers can pass file extensions or
        // human names interchangeably.
        self.syntaxes
            .find_syntax_by_extension(lang)
            .or_else(|| self.syntaxes.find_syntax_by_name(lang))
            .or_else(|| self.syntaxes.find_syntax_by_token(lang))
    }
}

fn style_to_rgb((style, text): (Style, &str)) -> (u32, String) {
    let c = style.foreground;
    let rgb = ((c.r as u32) << 16) | ((c.g as u32) << 8) | (c.b as u32);
    (rgb, text.to_owned())
}

/// Infer a syntect-friendly language token from a file path, matching the
/// SPA's `langForPath` map so colors agree across the apps.
pub fn lang_for_path(path: &str) -> Option<&'static str> {
    let ext = path.rsplit('.').next()?.to_lowercase();
    Some(match ext.as_str() {
        "rs" => "rs",
        "ts" | "tsx" => "ts",
        "js" | "jsx" => "js",
        "json" => "json",
        "toml" | "lock" => "toml",
        "yaml" | "yml" => "yaml",
        "md" => "md",
        "css" => "css",
        "html" => "html",
        "sh" | "bash" => "sh",
        "py" => "py",
        "go" => "go",
        _ => return None,
    })
}
