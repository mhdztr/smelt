//! Theme, glyph selection, color gating, and human units for the smelt CLI.
//!
//! Rules of record: DESIGN.md (glyph table, color precedence, human units)
//! and AGENTS.md ("color only through the `Theme` struct"; call sites never
//! apply styling unconditionally).

// Temporary: cli.rs/main.rs/the commands wire this up over the next few
// commits; the lint allow goes away with them.
#![allow(dead_code)]

use std::io::IsTerminal;

use clap::ValueEnum;
use owo_colors::OwoColorize;

/// When to colorize output (`--color <WHEN>`; DESIGN.md precedence).
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ColorChoice {
    /// Colorize when a stream is a terminal (default)
    Auto,
    /// Always colorize
    Always,
    /// Never colorize
    Never,
}

/// Everything printable goes through here: `color` gates ANSI styling,
/// `unicode` picks between the sanctioned glyphs and the ASCII fallbacks.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    pub color: bool,
    pub unicode: bool,
}

impl Theme {
    pub fn new(choice: ColorChoice) -> Self {
        let tty = std::io::stdout().is_terminal() || std::io::stderr().is_terminal();
        let no_color = std::env::var_os("NO_COLOR").is_some();
        let term = std::env::var_os("TERM").map(|t| t.to_string_lossy().into_owned());
        let locale = ["LC_ALL", "LC_CTYPE", "LANG"].iter().find_map(|key| {
            std::env::var_os(key)
                .filter(|v| !v.is_empty())
                .map(|v| v.to_string_lossy().into_owned())
        });
        let ascii_forced = std::env::var_os("SMELT_ASCII").is_some();
        Theme {
            color: resolve_color(choice, no_color, tty),
            unicode: unicode_mode(term.as_deref(), locale.as_deref(), ascii_forced, tty),
        }
    }

    /// Success / found mark: `✓` or `[ok]`.
    pub fn ok_mark(&self) -> &'static str {
        if self.unicode { "✓" } else { "[ok]" }
    }

    /// Failure / missing mark: `✗` or `[fail]`.
    pub fn fail_mark(&self) -> &'static str {
        if self.unicode { "✗" } else { "[fail]" }
    }

    /// Bullet: `▪` or `*`.
    pub fn bullet(&self) -> &'static str {
        if self.unicode { "▪" } else { "*" }
    }

    /// List-ish separator: `»` or `>`.
    pub fn sep(&self) -> &'static str {
        if self.unicode { "»" } else { ">" }
    }

    /// Horizontal rule: `─` or `-`.
    pub fn rule(&self) -> &'static str {
        if self.unicode { "─" } else { "-" }
    }

    /// Box-drawing corners and edges (summary box), ASCII `+ - |` fallback.
    pub fn box_chars(&self) -> BoxChars {
        if self.unicode {
            BoxChars {
                tl: "┌",
                tr: "┐",
                bl: "└",
                br: "┘",
                h: "─",
                v: "│",
            }
        } else {
            BoxChars {
                tl: "+",
                tr: "+",
                bl: "+",
                br: "+",
                h: "-",
                v: "|",
            }
        }
    }

    /// `{mark} {text}`, green when color is on.
    pub fn ok(&self, text: &str) -> String {
        let raw = format!("{} {}", self.ok_mark(), text);
        if self.color {
            raw.green().to_string()
        } else {
            raw
        }
    }

    /// `{mark} {text}`, red when color is on.
    pub fn fail(&self, text: &str) -> String {
        let raw = format!("{} {}", self.fail_mark(), text);
        if self.color {
            raw.red().to_string()
        } else {
            raw
        }
    }

    /// Bold (filenames).
    pub fn bold(&self, text: &str) -> String {
        if self.color {
            text.bold().to_string()
        } else {
            text.to_owned()
        }
    }

    /// Dim (secondary detail: sizes, timings, evidence, paths).
    pub fn dim(&self, text: &str) -> String {
        if self.color {
            text.dimmed().to_string()
        } else {
            text.to_owned()
        }
    }
}

/// The six box-drawing/ASCII characters used for framed output.
#[derive(Debug, Clone, Copy)]
pub struct BoxChars {
    pub tl: &'static str,
    pub tr: &'static str,
    pub bl: &'static str,
    pub br: &'static str,
    pub h: &'static str,
    pub v: &'static str,
}

/// Color precedence, highest first: `--color=never` > `NO_COLOR` (any
/// value) > `--color=always` > TTY autodetect. Pure core for testing;
/// `Theme::new` feeds it from the environment.
fn resolve_color(choice: ColorChoice, no_color: bool, tty: bool) -> bool {
    match choice {
        ColorChoice::Never => false,
        _ if no_color => false,
        ColorChoice::Always => true,
        ColorChoice::Auto => tty,
    }
}

/// Unicode glyphs require ALL of (DESIGN.md): a terminal, `TERM != linux`,
/// the first defined of `LC_ALL`/`LC_CTYPE`/`LANG` containing utf-8
/// (case-insensitive), and `SMELT_ASCII` unset. Pure core for testing.
fn unicode_mode(term: Option<&str>, locale: Option<&str>, ascii_forced: bool, tty: bool) -> bool {
    if ascii_forced || !tty || term == Some("linux") {
        return false;
    }
    locale.is_some_and(|l| {
        let lower = l.to_ascii_lowercase();
        lower.contains("utf-8") || lower.contains("utf8")
    })
}

/// Decimal byte units: `B` integer below 1 KB, one decimal place above
/// (`512 B`, `3.1 KB`, `2.1 MB` -- DESIGN.md human units).
pub fn human_bytes(bytes: u64) -> String {
    const KB: f64 = 1_000.0;
    const MB: f64 = 1_000_000.0;
    const GB: f64 = 1_000_000_000.0;
    let value = bytes as f64;
    if value < KB {
        format!("{bytes} B")
    } else if value < MB {
        format!("{:.1} KB", value / KB)
    } else if value < GB {
        format!("{:.1} MB", value / MB)
    } else {
        format!("{:.1} GB", value / GB)
    }
}

/// Durations: `120 ms` below one second, `1.4 s` below a minute, then
/// `2 m 05 s`.
pub fn human_duration(ms: u128) -> String {
    if ms < 1_000 {
        format!("{ms} ms")
    } else if ms < 60_000 {
        format!("{:.1} s", ms as f64 / 1_000.0)
    } else {
        let total = ms / 1_000;
        format!("{} m {:02} s", total / 60, total % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_precedence_follows_design() {
        // --color=never beats everything.
        assert!(!resolve_color(ColorChoice::Never, false, true));
        assert!(!resolve_color(ColorChoice::Never, true, true));
        // NO_COLOR beats --color=always.
        assert!(!resolve_color(ColorChoice::Always, true, false));
        assert!(!resolve_color(ColorChoice::Auto, true, true));
        // --color=always beats TTY autodetect (and works off-TTY).
        assert!(resolve_color(ColorChoice::Always, false, false));
        // Auto follows the terminal.
        assert!(resolve_color(ColorChoice::Auto, false, true));
        assert!(!resolve_color(ColorChoice::Auto, false, false));
    }

    #[test]
    fn unicode_needs_all_conditions() {
        let locale = Some("en_US.UTF-8");
        assert!(unicode_mode(Some("xterm-256color"), locale, false, true));
        // Any single missing condition disables it.
        assert!(!unicode_mode(Some("linux"), locale, false, true));
        assert!(!unicode_mode(Some("xterm"), locale, false, false));
        assert!(!unicode_mode(Some("xterm"), locale, true, true));
        assert!(!unicode_mode(Some("xterm"), None, false, true));
        assert!(!unicode_mode(Some("xterm"), Some("C"), false, true));
    }

    #[test]
    fn utf8_locale_matching_is_case_insensitive() {
        assert!(unicode_mode(Some("xterm"), Some("en_US.utf8"), false, true));
        assert!(unicode_mode(Some("xterm"), Some("en_US.UTF8"), false, true));
        assert!(!unicode_mode(
            Some("xterm"),
            Some("en_US.ISO-8859-1"),
            false,
            true
        ));
    }

    #[test]
    fn human_bytes_uses_decimal_units() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(512), "512 B");
        assert_eq!(human_bytes(3_100), "3.1 KB");
        assert_eq!(human_bytes(3481), "3.5 KB");
        assert_eq!(human_bytes(2_100_000), "2.1 MB");
        assert_eq!(human_bytes(1_500_000_000), "1.5 GB");
    }

    #[test]
    fn human_duration_scales() {
        assert_eq!(human_duration(120), "120 ms");
        assert_eq!(human_duration(999), "999 ms");
        assert_eq!(human_duration(1_400), "1.4 s");
        assert_eq!(human_duration(125_000), "2 m 05 s");
    }

    #[test]
    fn ascii_theme_uses_fallback_glyphs() {
        let theme = Theme {
            color: false,
            unicode: false,
        };
        assert_eq!(theme.ok_mark(), "[ok]");
        assert_eq!(theme.fail_mark(), "[fail]");
        assert_eq!(theme.bullet(), "*");
        assert_eq!(theme.sep(), ">");
        assert_eq!(theme.rule(), "-");
        let b = theme.box_chars();
        assert_eq!((b.tl, b.h, b.v), ("+", "-", "|"));
        assert_eq!(theme.ok("done"), "[ok] done");
        assert_eq!(theme.fail("nope"), "[fail] nope");
        assert_eq!(theme.bold("a.md"), "a.md");
        assert_eq!(theme.dim("3.1 KB"), "3.1 KB");
    }

    #[test]
    fn unicode_theme_uses_sanctioned_glyphs() {
        let theme = Theme {
            color: false,
            unicode: true,
        };
        assert_eq!(theme.ok("done"), "✓ done");
        assert_eq!(theme.fail("nope"), "✗ nope");
        assert_eq!(theme.bullet(), "▪");
        assert_eq!(theme.sep(), "»");
        assert_eq!(theme.rule(), "─");
        let b = theme.box_chars();
        assert_eq!(
            (b.tl, b.tr, b.bl, b.br, b.h, b.v),
            ("┌", "┐", "└", "┘", "─", "│")
        );
    }
}
