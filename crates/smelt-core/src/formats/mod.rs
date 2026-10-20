//! Native format converters. One module per format; each pairs a `Reader`
//! and a `Writer` over the IDM. Registration happens in
//! `crate::{reader_for, writer_for}`.
//!
//! This module also hosts whitespace/text helpers shared by the html and
//! plain-text converters.

pub mod html;
pub mod markdown;
pub mod text;

use crate::idm::Inline;

/// Whether a character counts as collapsible whitespace. Non-breaking space
/// (U+00A0) is content, not whitespace, and is preserved by every converter.
pub(crate) fn is_ws(c: char) -> bool {
    c.is_whitespace() && c != '\u{a0}'
}

/// Collapse every whitespace run to a single space (HTML rendering
/// semantics). NBSP is kept verbatim. Edges are *not* trimmed; callers
/// that need trimming use [`trim_ws`].
pub(crate) fn collapse_ws(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut in_ws = false;
    for c in s.chars() {
        if is_ws(c) {
            if !in_ws {
                out.push(' ');
                in_ws = true;
            }
        } else {
            out.push(c);
            in_ws = false;
        }
    }
    out
}

/// Trim leading and trailing collapsible whitespace, keeping NBSP.
pub(crate) fn trim_ws(s: &str) -> &str {
    s.trim_matches(is_ws)
}

/// Merge adjacent `Text` runs in place. Needed because empty inline
/// containers (e.g. `<strong></strong>`) drop out, which can leave two text
/// runs next to each other.
pub(crate) fn merge_text(inlines: &mut Vec<Inline>) {
    let mut merged: Vec<Inline> = Vec::with_capacity(inlines.len());
    for inline in inlines.drain(..) {
        match (merged.last_mut(), inline) {
            (Some(Inline::Text(prev)), Inline::Text(next)) => prev.push_str(&next),
            (_, other) => merged.push(other),
        }
    }
    *inlines = merged;
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn collapse_ws_keeps_nbsp_and_collapses_runs() {
        assert_eq!(collapse_ws("a \t\n b"), "a b");
        assert_eq!(collapse_ws("\u{a0}keep\u{a0}"), "\u{a0}keep\u{a0}");
        assert_eq!(collapse_ws("   "), " ");
        assert_eq!(collapse_ws(""), "");
    }

    #[test]
    fn trim_ws_leaves_nbsp_at_edges() {
        assert_eq!(trim_ws(" \u{a0}x\u{a0} "), "\u{a0}x\u{a0}");
        assert_eq!(trim_ws("  x  "), "x");
    }

    #[test]
    fn merge_text_joins_adjacent_runs() {
        let mut inlines = vec![
            Inline::Text("a".to_owned()),
            Inline::Text("b".to_owned()),
            Inline::Emph(vec![Inline::Text("c".to_owned())]),
            Inline::Text("d".to_owned()),
        ];
        merge_text(&mut inlines);
        assert_eq!(
            inlines,
            vec![
                Inline::Text("ab".to_owned()),
                Inline::Emph(vec![Inline::Text("c".to_owned())]),
                Inline::Text("d".to_owned()),
            ]
        );
    }
}

/// Cross-format roundtrips compare `Document` values across converters
/// (AGENTS.md testing rule), never raw text.
#[cfg(test)]
mod cross_format_tests {
    use smelt_detect::Format;

    use crate::Reader;
    use crate::Writer;
    use crate::idm::Document;

    use super::html::{HtmlReader, HtmlWriter};
    use super::markdown::{MarkdownReader, MarkdownWriter};
    use super::text::{TextReader, TextWriter};

    fn read_md(src: &str) -> Document {
        MarkdownReader
            .read(src.as_bytes(), Format::Markdown)
            .unwrap()
    }
    fn write_md(doc: &Document) -> String {
        String::from_utf8(MarkdownWriter.write(doc, Format::Markdown).unwrap()).unwrap()
    }
    fn read_html(src: &str) -> Document {
        HtmlReader.read(src.as_bytes(), Format::Html).unwrap()
    }
    fn write_html(doc: &Document) -> String {
        String::from_utf8(HtmlWriter.write(doc, Format::Html).unwrap()).unwrap()
    }
    fn read_txt(src: &str) -> Document {
        TextReader.read(src.as_bytes(), Format::Text).unwrap()
    }
    fn write_txt(doc: &Document) -> String {
        String::from_utf8(TextWriter.write(doc, Format::Text).unwrap()).unwrap()
    }

    /// Fixtures chosen to exercise the loss-prone seams: front matter via
    /// title/author metadata, table alignment via `align` attributes,
    /// block-level `Image` back as `<img>`, RTL/Arabic with combining
    /// marks, and three-level nested lists.
    fn md_fixtures() -> Vec<(&'static str, &'static str)> {
        vec![
            (
                "front matter and heading",
                "---\ntitle: Doc Title\nauthor: A. Author\n---\n\n# Head\n\nbody\n",
            ),
            (
                "rtl arabic with combining marks",
                "## \u{0645}\u{064f}\u{0644}\u{062e}\u{0637}\n\nThe text \u{0627}\u{0644}\u{0639}\u{0631}\u{0628}\u{064a}\u{0629} \u{0645}\u{062f}\u{0642}\u{0648}\u{0637} stays intact.\n",
            ),
            (
                "gfm table with alignment",
                "| left | center | right |\n| :--- | :---: | ---: |\n| a | b | c |\n| \u{0627}\u{0644}\u{0639}\u{0631}\u{0628}\u{064a}\u{0629} | mid | 42 |\n",
            ),
            (
                "three level nested list",
                "1. one\n2. two\n   - a\n   - b\n     1. deep\n     2. deeper\n3. three\n",
            ),
            (
                "code block with lang",
                "before\n\n```rust\nfn main() {\n    println!(\"hi\");\n}\n```\n\nafter\n",
            ),
            (
                "strikethrough and autolink",
                "~~gone~~ kept, see https://example.com/docs now\n",
            ),
            (
                "blockquote containing a list",
                "> quoted intro\n>\n> - item one\n> - item two\n",
            ),
            (
                "emphasis nesting and inline code",
                "**bold and *italic* together** plus `a_b` code\n",
            ),
            (
                "rule and image",
                "intro\n\n---\n\n![an alt](img/pic.png)\n\noutro\n",
            ),
            (
                "links and lists together",
                "See [the docs](https://example.com \"Docs\") and:\n\n- first\n- second\n",
            ),
        ]
    }

    #[test]
    fn md_html_md_is_document_equal() {
        for (name, src) in md_fixtures() {
            let doc_md = read_md(src);
            let doc_html = read_html(&write_html(&doc_md));
            assert_eq!(
                doc_md,
                doc_html,
                "md->html leg diverged: {name}\n--- html ---\n{}",
                write_html(&doc_md)
            );
            let back = read_md(&write_md(&doc_html));
            assert_eq!(
                doc_md,
                back,
                "md->html->md leg diverged: {name}\n--- md ---\n{}",
                write_md(&doc_html)
            );
        }
    }

    #[test]
    fn html_md_html_is_document_equal() {
        let src = "<!DOCTYPE html>\n<html>\n<head>\n<title>Doc Title</title>\n\
                   <meta name=\"author\" content=\"A. Author\">\n</head>\n<body>\n\
                   <h2>Head</h2>\n\
                   <p>para with <strong>b</strong>, <em>i</em>, <del>s</del>, \
                   <code>c</code> and <a href=\"https://example.com\">link</a></p>\n\
                   <ul><li>one</li><li>two<ul><li>inner</li></ul></li></ul>\n\
                   <ol start=\"3\"><li>three</li></ol>\n\
                   <table><tr><th align=\"center\">h</th></tr>\
                   <tr><td>\u{0627}\u{0644}\u{0639}\u{0631}\u{0628}\u{064a}\u{0629}</td></tr></table>\n\
                   <pre><code class=\"language-rust\">fn f() {}</code></pre>\n\
                   <blockquote><p>quoted</p></blockquote>\n\
                   <hr>\n<img src=\"p.png\" alt=\"alt\">\n</body>\n</html>\n";
        let doc_html = read_html(src);
        let doc_md = read_md(&write_md(&doc_html));
        assert_eq!(
            doc_html,
            doc_md,
            "html->md leg diverged\n--- md ---\n{}",
            write_md(&doc_html)
        );
        let back = read_html(&write_html(&doc_md));
        assert_eq!(
            doc_html,
            back,
            "html->md->html leg diverged\n--- html ---\n{}",
            write_html(&doc_md)
        );
    }

    #[test]
    fn txt_md_txt_is_document_equal() {
        let src = "# not a heading\n\nplain paragraph with  spaces and 2 * 3 = 6 stars\n\n\
                   \u{0627}\u{0644}\u{0639}\u{0631}\u{0628}\u{064a}\u{0629} paragraph\n";
        let doc_txt = read_txt(src);
        let doc_md = read_md(&write_md(&doc_txt));
        assert_eq!(
            doc_txt,
            doc_md,
            "txt->md leg diverged\n--- md ---\n{}",
            write_md(&doc_txt)
        );
        let back = read_txt(&write_txt(&doc_md));
        assert_eq!(
            doc_txt,
            back,
            "txt->md->txt leg diverged\n--- txt ---\n{}",
            write_txt(&doc_md)
        );
    }
}
