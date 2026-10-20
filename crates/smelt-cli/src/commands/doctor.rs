//! `smelt doctor`: report optional external engines found on PATH.
//! Always exits 0 (docs/cli.md); never runs on the convert path
//! (PERFORMANCE.md guardrail 1).

use crate::SmeltError;
use crate::engine::{ENGINES, probe};
use crate::ui::Theme;

/// Name column width from DESIGN.md's normative sample (`soffice` padded
/// so every path starts in the same column).
const NAME_WIDTH: usize = 14;

pub fn run(theme: &Theme) -> Result<(), SmeltError> {
    for name in ENGINES {
        let engine = probe(name);
        let (found, detail) = match (&engine.path, &engine.version) {
            // Found: path, optionally with the --version banner. Paths are
            // secondary info (DESIGN.md color rules): dim.
            (Some(path), Some(version)) => {
                (true, theme.dim(&format!("{} ({version})", path.display())))
            }
            (Some(path), None) => (true, theme.dim(&path.display().to_string())),
            (None, _) => (false, "not found (optional)".to_string()),
        };
        println!("{}", row(theme, found, engine.name, &detail));
    }

    println!();
    println!("smelt M0 uses no external engines; these are reported for future milestones.");
    Ok(())
}

/// One doctor row. The mark goes through `Theme::ok`/`Theme::fail`
/// (DESIGN.md: success green `✓`, failure red `✗`); the name is padded
/// to `NAME_WIDTH`; `detail` arrives already styled by the caller.
fn row(theme: &Theme, found: bool, name: &str, detail: &str) -> String {
    let mark = if found { theme.ok("") } else { theme.fail("") };
    format!("{mark}{name:name_width$}{detail}", name_width = NAME_WIDTH)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme(color: bool, unicode: bool) -> Theme {
        Theme { color, unicode }
    }

    #[test]
    fn row_layout_matches_docs_sample() {
        // docs/cli.md: `✓ soffice       /usr/bin/soffice (LibreOffice 26.8.1.1)`
        let t = theme(false, true);
        assert_eq!(
            row(
                &t,
                true,
                "soffice",
                "/usr/bin/soffice (LibreOffice 26.8.1.1)"
            ),
            "✓ soffice       /usr/bin/soffice (LibreOffice 26.8.1.1)"
        );
        // docs/cli.md: `✗ pandoc        not found (optional)`
        assert_eq!(
            row(&t, false, "pandoc", "not found (optional)"),
            "✗ pandoc        not found (optional)"
        );
    }

    #[test]
    fn row_marks_are_colored_through_theme() {
        // DESIGN.md: success green, failure red -- only when color is on.
        let on = theme(true, true);
        let green = row(&on, true, "soffice", "/usr/bin/soffice");
        let red = row(&on, false, "pandoc", "not found (optional)");
        assert!(green.contains("\u{1b}[32m"), "green mark: {green:?}");
        assert!(red.contains("\u{1b}[31m"), "red mark: {red:?}");

        let off = theme(false, true);
        assert!(!row(&off, true, "soffice", "/x").contains('\u{1b}'));
        assert!(!row(&off, false, "pandoc", "not found (optional)").contains('\u{1b}'));
    }

    #[test]
    fn row_ascii_fallback() {
        let t = theme(false, false);
        assert_eq!(
            row(&t, true, "tesseract", "/usr/bin/tesseract"),
            "[ok] tesseract     /usr/bin/tesseract"
        );
        assert_eq!(
            row(&t, false, "pandoc", "not found (optional)"),
            "[fail] pandoc        not found (optional)"
        );
    }
}
