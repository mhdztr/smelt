//! Plain-text converter.
//!
//! Reader: blank-line paragraph splitting. Every block of non-blank lines
//! becomes one `Paragraph`; whitespace inside a block collapses to single
//! spaces. Nothing else is guessed -- headings, lists, and tables in plain
//! text are indistinguishable from prose (FORMATS.md lossy list).
//!
//! Writer: IDM -> plain text. Structure is rendered minimally: ATX-prefixed
//! headings, `- `/`1. ` lists with a two-space nesting indent, space-aligned
//! tables (widths via unicode-width so wide characters line up), links as
//! `text (dest)`, rules as `---`. Metadata has no plain-text channel and
//! drops.

use smelt_detect::Format;

use unicode_width::UnicodeWidthStr;

use crate::error::{ReadError, WriteError};
use crate::idm::{Align, Block, Document, Inline, Metadata};
use crate::{Reader, Writer};

use super::{collapse_ws, trim_ws};

/// The plain-text reader (blank-line paragraph split -> IDM).
pub struct TextReader;

/// The plain-text writer (IDM -> plain text).
pub struct TextWriter;

impl Reader for TextReader {
    fn read(&self, input: &[u8], _fmt: Format) -> Result<Document, ReadError> {
        let text = std::str::from_utf8(input).map_err(|_| ReadError::NotUtf8)?;
        let mut blocks = Vec::new();
        for chunk in paragraph_chunks(text) {
            let collapsed = collapse_ws(trim_ws(&chunk));
            if collapsed.is_empty() {
                continue;
            }
            blocks.push(Block::Paragraph {
                inlines: vec![Inline::Text(collapsed)],
            });
        }
        Ok(Document {
            meta: Metadata::default(),
            blocks,
        })
    }
}

/// Group consecutive non-blank lines into space-joined chunks, one per
/// paragraph, dropping blank lines. `str::lines` already strips the `\r`
/// of `\r\n` files.
fn paragraph_chunks(text: &str) -> Vec<String> {
    let mut chunks: Vec<String> = Vec::new();
    let mut current: Option<String> = None;
    for line in text.lines() {
        if trim_ws(line).is_empty() {
            if let Some(chunk) = current.take() {
                chunks.push(chunk);
            }
        } else {
            let entry = current.get_or_insert_with(String::new);
            if !entry.is_empty() {
                entry.push(' ');
            }
            entry.push_str(line);
        }
    }
    if let Some(chunk) = current {
        chunks.push(chunk);
    }
    chunks
}

impl Writer for TextWriter {
    fn write(&self, doc: &Document, _fmt: Format) -> Result<Vec<u8>, WriteError> {
        // Metadata has no plain-text channel; it drops by design.
        let mut out = String::new();
        for (i, block) in doc.blocks.iter().enumerate() {
            if i > 0 {
                out.push_str("\n\n");
            }
            write_block(block, &mut out);
        }
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        Ok(out.into_bytes())
    }
}

// ---------------------------------------------------------------------------
// Writer helpers
// ---------------------------------------------------------------------------

fn write_block(block: &Block, out: &mut String) {
    match block {
        Block::Heading { level, inlines } => {
            let level = (*level).clamp(1, 6) as usize;
            out.push_str(&"#".repeat(level));
            out.push(' ');
            out.push_str(&inlines_text(inlines));
        }
        Block::Paragraph { inlines } => out.push_str(&inlines_text(inlines)),
        Block::List {
            ordered,
            start,
            items,
        } => {
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push('\n');
                }
                let marker = if *ordered {
                    format!("{}. ", start + i as u64)
                } else {
                    "- ".to_owned()
                };
                write_list_item(item, &marker, out);
            }
        }
        Block::Table {
            header,
            rows,
            align,
        } => write_table(header, rows, align, out),
        Block::CodeBlock { text, .. } => {
            // The language tag has no plain-text rendering; the content is
            // emitted verbatim (one trailing newline stripped -- the block
            // joiner re-adds separation).
            out.push_str(text.strip_suffix('\n').unwrap_or(text));
        }
        Block::Image { src, alt } => out.push_str(&format!("image: {alt} ({src})")),
        Block::Rule => out.push_str("---"),
        Block::Blockquote { blocks } => {
            let mut piece = String::new();
            for (i, block) in blocks.iter().enumerate() {
                if i > 0 {
                    piece.push_str("\n\n");
                }
                write_block(block, &mut piece);
            }
            for (i, line) in piece.lines().enumerate() {
                if i > 0 {
                    out.push('\n');
                }
                out.push('>');
                if !line.is_empty() {
                    out.push(' ');
                    out.push_str(line);
                }
            }
        }
    }
}

/// List item: the first line continues on the marker, later lines align
/// under it (the two-space nesting indent for `- ` markers). A nested list
/// gets its own marker through [`write_block`]; the alignment comes from the
/// outer prefix length, so `10. ` items line up too.
fn write_list_item(item: &[Block], marker: &str, out: &mut String) {
    // Blocks join with a single newline: a blank line would terminate the
    // item for any reader that re-parses this output.
    let mut piece = String::new();
    for (i, block) in item.iter().enumerate() {
        if i > 0 {
            piece.push('\n');
        }
        write_block(block, &mut piece);
    }
    if piece.is_empty() {
        out.push_str(marker.trim_end());
        return;
    }
    let mut lines = piece.lines();
    let first = lines.next().unwrap_or("");
    out.push_str(marker);
    out.push_str(first);
    for line in lines {
        out.push('\n');
        if !line.is_empty() {
            out.push_str(&" ".repeat(marker.len()));
            out.push_str(line);
        }
    }
}

/// Space-align table columns on display width (unicode-width), two spaces
/// between columns, a `-` underline row after the header.
fn write_table(
    header: &[Vec<Inline>],
    rows: &[Vec<Vec<Inline>>],
    align: &[Align],
    out: &mut String,
) {
    let cols = header.len();
    if cols == 0 {
        return;
    }
    let mut grid: Vec<Vec<String>> = Vec::with_capacity(rows.len() + 1);
    grid.push(header.iter().map(|c| inlines_text(c)).collect());
    for row in rows {
        let mut cells: Vec<String> = row.iter().map(|c| inlines_text(c)).collect();
        cells.resize(cols, String::new());
        grid.push(cells);
    }
    let mut widths: Vec<usize> = vec![0; cols];
    for row in &grid {
        for (i, cell) in row.iter().enumerate() {
            widths[i] = widths[i].max(cell.width());
        }
    }
    let render = |row: &[String]| {
        let mut parts = Vec::with_capacity(cols);
        for (i, cell) in row.iter().enumerate() {
            let pad = widths[i].saturating_sub(cell.width());
            let (before, after) = match align.get(i) {
                Some(Align::Right) => (pad, 0),
                Some(Align::Center) => (pad / 2, pad - pad / 2),
                _ => (0, pad),
            };
            parts.push(format!(
                "{}{}{}",
                " ".repeat(before),
                cell,
                " ".repeat(after)
            ));
        }
        parts.join("  ")
    };
    for (i, row) in grid.iter().enumerate() {
        if i > 0 {
            out.push('\n');
        }
        out.push_str(&render(row));
        if i == 0 {
            out.push('\n');
            let underline: Vec<String> = widths.iter().map(|w| "-".repeat(*w)).collect();
            out.push_str(&underline.join("  "));
        }
    }
}

/// Render an inline run to plain text: emphasis and strike markers drop,
/// links become `text (dest)`.
fn inlines_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) | Inline::Code(t) => out.push_str(t),
            Inline::Strong(inner) | Inline::Emph(inner) | Inline::Strikethrough(inner) => {
                out.push_str(&inlines_text(inner));
            }
            Inline::Link { dest, inlines, .. } => {
                out.push_str(&inlines_text(inlines));
                out.push_str(&format!(" ({dest})"));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_txt(src: &str) -> Document {
        TextReader.read(src.as_bytes(), Format::Text).unwrap()
    }

    fn write_txt(doc: &Document) -> String {
        String::from_utf8(TextWriter.write(doc, Format::Text).unwrap()).unwrap()
    }

    fn para(text: &str) -> Block {
        Block::Paragraph {
            inlines: vec![Inline::Text(text.to_owned())],
        }
    }

    #[test]
    fn paragraphs_split_on_blank_lines_and_collapse() {
        let doc = parse_txt("one line\nstill one\n\n  two \t spaced   lines \n\n\nthree\n");
        assert_eq!(
            doc.blocks,
            vec![
                para("one line still one"),
                para("two spaced lines"),
                para("three"),
            ]
        );
    }

    #[test]
    fn crlf_input_behaves_like_lf() {
        let doc = parse_txt("a\r\nb\r\n\r\nc\r\n");
        assert_eq!(doc.blocks, vec![para("a b"), para("c")]);
    }

    #[test]
    fn rtl_arabic_survives_read_and_write() {
        let arabic = "\u{0627}\u{0644}\u{0639}\u{0631}\u{0628}\u{064a}\u{0629}";
        let doc = parse_txt(&format!("Some {arabic} text\n"));
        assert_eq!(write_txt(&doc), format!("Some {arabic} text\n"));
    }

    #[test]
    fn text_roundtrips_through_itself() {
        for src in [
            "para one\n\npara two with  spaces\n",
            "\u{0645}\u{0644}\u{062e}\u{0637} \u{0648}\u{0627}\u{062d}\u{062f}\n",
            "   \n\nonly one paragraph\n",
        ] {
            let once = parse_txt(src);
            let twice = parse_txt(&write_txt(&once));
            assert_eq!(once, twice, "txt->txt roundtrip diverged for {src:?}");
        }
    }

    #[test]
    fn lists_indent_nesting_by_two_spaces() {
        let doc = Document {
            meta: Metadata::default(),
            blocks: vec![Block::List {
                ordered: false,
                start: 0,
                items: vec![
                    vec![para("one")],
                    vec![
                        para("two"),
                        Block::List {
                            ordered: true,
                            start: 1,
                            items: vec![vec![para("deep")]],
                        },
                    ],
                ],
            }],
        };
        assert_eq!(write_txt(&doc), "- one\n- two\n  1. deep\n");
    }

    #[test]
    fn ordered_markers_align_under_the_number() {
        let doc = Document {
            meta: Metadata::default(),
            blocks: vec![Block::List {
                ordered: true,
                start: 9,
                items: vec![vec![para("nine")], vec![para("ten")]],
            }],
        };
        assert_eq!(write_txt(&doc), "9. nine\n10. ten\n");
    }

    #[test]
    fn links_become_text_with_dest() {
        let doc = Document {
            meta: Metadata::default(),
            blocks: vec![Block::Paragraph {
                inlines: vec![
                    Inline::Text("see ".to_owned()),
                    Inline::Link {
                        dest: "https://example.com".to_owned(),
                        title: None,
                        inlines: vec![Inline::Text("the docs".to_owned())],
                    },
                ],
            }],
        };
        assert_eq!(write_txt(&doc), "see the docs (https://example.com)\n");
    }

    #[test]
    fn tables_align_on_display_width() {
        let doc = Document {
            meta: Metadata::default(),
            blocks: vec![Block::Table {
                header: vec![
                    vec![Inline::Text("name".to_owned())],
                    vec![Inline::Text("n".to_owned())],
                ],
                rows: vec![vec![
                    vec![Inline::Text("\u{0645}\u{0644}\u{062e}\u{0637}".to_owned())],
                    vec![Inline::Text("42".to_owned())],
                ]],
                align: vec![Align::None, Align::Right],
            }],
        };
        let text = write_txt(&doc);
        let widths: Vec<usize> = text.lines().map(UnicodeWidthStr::width).collect();
        assert!(
            widths.windows(2).all(|w| w[0] == w[1]),
            "lines not column-aligned: {text:?} ({widths:?})"
        );
    }

    #[test]
    fn rule_and_heading_render_minimally() {
        let doc = Document {
            meta: Metadata::default(),
            blocks: vec![
                Block::Heading {
                    level: 2,
                    inlines: vec![Inline::Text("head".to_owned())],
                },
                Block::Rule,
            ],
        };
        assert_eq!(write_txt(&doc), "## head\n\n---\n");
    }

    #[test]
    fn blockquote_prefixes_every_line() {
        let doc = Document {
            meta: Metadata::default(),
            blocks: vec![Block::Blockquote {
                blocks: vec![para("line one"), para("line two")],
            }],
        };
        assert_eq!(write_txt(&doc), "> line one\n>\n> line two\n");
    }

    #[test]
    fn not_utf8_is_an_actionable_error() {
        let err = TextReader.read(&[0xff], Format::Text).unwrap_err();
        assert!(matches!(err, ReadError::NotUtf8));
    }
}
