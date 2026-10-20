//! Markdown reader and writer built on comrak.
//!
//! Reader: comrak AST -> IDM. GFM tables, strikethrough, and autolinks are
//! enabled; task lists are disabled (IDM v0 has no checkbox node; `- [ ]`
//! degrades to plain list-item text -- see FORMATS.md lossy list). A leading
//! `---` ... `---` front matter block carries `title`/`author` into IDM
//! metadata. Raw HTML blocks parse into structure through the html
//! converter's fragment walker; inline HTML tags are dropped (IDM v0 has no
//! raw-inline node -- FORMATS.md lossy list).
//!
//! Writer: IDM -> markdown. Minimal escaping keeps literal text runs from
//! re-parsing as structure. Roundtrip tests compare `Document` values, never
//! raw text (AGENTS.md testing rule).

use comrak::Arena;
use comrak::nodes::{AstNode, ListType, NodeValue};
use comrak::{Options, parse_document};

use smelt_detect::Format;

use crate::error::{ReadError, WriteError};
use crate::idm::{Align, Block, Document, Inline, Metadata};
use crate::{Reader, Writer};

/// The markdown reader (comrak AST -> IDM).
pub struct MarkdownReader;

/// The markdown writer (IDM -> markdown).
pub struct MarkdownWriter;

/// comrak options matching the IDM v0 contract (FORMATS.md, idm-spec.md).
fn parse_options() -> Options<'static> {
    let mut opts = Options::default();
    opts.extension.table = true;
    opts.extension.strikethrough = true;
    opts.extension.autolink = true;
    // Deliberately off: IDM v0 has no checkbox node, so task items stay
    // plain list items with their literal `[ ]` text (lossy, documented).
    opts.extension.tasklist = false;
    // Front matter is the md metadata channel (title/author).
    opts.extension.front_matter_delimiter = Some("---".to_owned());
    opts
}

struct Converter {
    meta: Metadata,
}

impl Reader for MarkdownReader {
    fn read(&self, input: &[u8], _fmt: Format) -> Result<Document, ReadError> {
        let text = std::str::from_utf8(input).map_err(|_| ReadError::NotUtf8)?;
        let arena = Arena::new();
        let root = parse_document(&arena, text, &parse_options());
        let mut converter = Converter {
            meta: Metadata::default(),
        };
        let blocks = converter.blocks(root);
        Ok(Document {
            meta: converter.meta,
            blocks,
        })
    }
}

impl Converter {
    /// Convert the children of a block container into IDM blocks.
    fn blocks<'a>(&mut self, node: &'a AstNode<'a>) -> Vec<Block> {
        let mut out = Vec::new();
        for child in node.children() {
            match &child.data().value {
                NodeValue::FrontMatter(raw) => parse_front_matter(raw, &mut self.meta),
                NodeValue::Heading(h) => {
                    let level = h.level.clamp(1, 6);
                    let (inlines, images) = inlines(child);
                    out.push(Block::Heading { level, inlines });
                    out.extend(images);
                }
                NodeValue::Paragraph => {
                    // Inline images are promoted to a following Image block
                    // (idm-spec.md), in document order.
                    let mut current: Vec<Inline> = Vec::new();
                    for item in child.children() {
                        if matches!(item.data().value, NodeValue::Image(_)) {
                            if !current.is_empty() {
                                out.push(Block::Paragraph {
                                    inlines: std::mem::take(&mut current),
                                });
                            }
                            push_image_block(item, &mut out);
                            continue;
                        }
                        push_inline(item, &mut current, &mut out);
                    }
                    if !current.is_empty() {
                        out.push(Block::Paragraph { inlines: current });
                    }
                }
                NodeValue::List(list) => {
                    let ordered = matches!(list.list_type, ListType::Ordered);
                    let start = if ordered { list.start.max(1) as u64 } else { 0 };
                    let items = child.children().map(|item| self.blocks(item)).collect();
                    out.push(Block::List {
                        ordered,
                        start,
                        items,
                    });
                }
                NodeValue::Table(table) => {
                    let mut header: Vec<Vec<Inline>> = Vec::new();
                    let mut rows: Vec<Vec<Vec<Inline>>> = Vec::new();
                    for row in child.children() {
                        let cells: Vec<Vec<Inline>> =
                            row.children().map(|cell| inlines(cell).0).collect();
                        match row.data().value {
                            NodeValue::TableRow(true) => header = cells,
                            _ => rows.push(cells),
                        }
                    }
                    let align = table.alignments.iter().map(align_from_comrak).collect();
                    out.push(Block::Table {
                        header,
                        rows,
                        align,
                    });
                }
                NodeValue::CodeBlock(cb) => {
                    let lang = cb.info.split_whitespace().next().map(str::to_owned);
                    out.push(Block::CodeBlock {
                        lang,
                        text: cb.literal.clone(),
                    });
                }
                NodeValue::ThematicBreak => out.push(Block::Rule),
                NodeValue::BlockQuote => {
                    out.push(Block::Blockquote {
                        blocks: self.blocks(child),
                    });
                }
                NodeValue::HtmlBlock(html) => {
                    // Raw HTML blocks parse into structure via the html
                    // converter's fragment walker (same rules as html input;
                    // metadata is not extracted from fragments).
                    out.extend(crate::formats::html::fragment_blocks(&html.literal));
                }
                // Containers reachable only through their parents.
                NodeValue::Item(_) | NodeValue::TableRow(_) | NodeValue::TableCell => {
                    out.extend(self.blocks(child));
                }
                // Anything else at block level: unwrap and keep the blocks.
                _ => out.extend(self.blocks(child)),
            }
        }
        out
    }
}

fn align_from_comrak(a: &comrak::nodes::TableAlignment) -> Align {
    match a {
        comrak::nodes::TableAlignment::None => Align::None,
        comrak::nodes::TableAlignment::Left => Align::Left,
        comrak::nodes::TableAlignment::Center => Align::Center,
        comrak::nodes::TableAlignment::Right => Align::Right,
    }
}

/// Collect the inline content of a container plus any images promoted to
/// blocks (nested images land after the containing block).
fn inlines<'a>(node: &'a AstNode<'a>) -> (Vec<Inline>, Vec<Block>) {
    let mut out = Vec::new();
    let mut images = Vec::new();
    for child in node.children() {
        push_inline(child, &mut out, &mut images);
    }
    merge_text(&mut out);
    (out, images)
}

/// Append the IDM inline for one comrak node, or record a promoted image.
fn push_inline<'a>(node: &'a AstNode<'a>, out: &mut Vec<Inline>, images: &mut Vec<Block>) {
    match &node.data().value {
        NodeValue::Text(t) => out.push(Inline::Text(t.to_string())),
        // Soft and hard breaks both collapse to a space: the IDM v0 inline
        // set has no break node, and a space roundtrips identically through
        // html and plain text (documented in FORMATS.md).
        NodeValue::SoftBreak | NodeValue::LineBreak => out.push(Inline::Text(" ".to_owned())),
        NodeValue::Code(c) => out.push(Inline::Code(c.literal.clone())),
        NodeValue::Emph => {
            let inner = children_inlines(node);
            if !inner.is_empty() {
                out.push(Inline::Emph(inner));
            }
        }
        NodeValue::Strong => {
            let inner = children_inlines(node);
            if !inner.is_empty() {
                out.push(Inline::Strong(inner));
            }
        }
        NodeValue::Strikethrough => {
            let inner = children_inlines(node);
            if !inner.is_empty() {
                out.push(Inline::Strikethrough(inner));
            }
        }
        NodeValue::Link(link) => {
            let inner = children_inlines(node);
            if !inner.is_empty() {
                out.push(Inline::Link {
                    dest: link.url.clone(),
                    title: (!link.title.is_empty()).then(|| link.title.clone()),
                    inlines: inner,
                });
            }
        }
        NodeValue::Image(_) => push_image_block(node, images),
        NodeValue::HtmlInline(_) => {
            // Inline HTML drops: comrak parses tags into separate HtmlInline
            // nodes, and the IDM v0 inline set has no raw-passthrough node
            // (documented in FORMATS.md).
        }
        _ => {}
    }
}

fn children_inlines<'a>(node: &'a AstNode<'a>) -> Vec<Inline> {
    let mut out = Vec::new();
    let mut images = Vec::new();
    for child in node.children() {
        push_inline(child, &mut out, &mut images);
    }
    merge_text(&mut out);
    out
}

fn push_image_block<'a>(node: &'a AstNode<'a>, out: &mut Vec<Block>) {
    let NodeValue::Image(link) = &node.data().value else {
        return;
    };
    let mut alt = String::new();
    collect_text(node, &mut alt);
    out.push(Block::Image {
        src: link.url.clone(),
        alt,
    });
}

fn collect_text<'a>(node: &'a AstNode<'a>, out: &mut String) {
    if let NodeValue::Text(t) = &node.data().value {
        out.push_str(t);
    }
    for child in node.children() {
        collect_text(child, out);
    }
}

/// Merge adjacent text runs (comrak splits around entities and breaks).
fn merge_text(inlines: &mut Vec<Inline>) {
    let mut merged: Vec<Inline> = Vec::with_capacity(inlines.len());
    for inline in std::mem::take(inlines) {
        if let Inline::Text(t) = inline {
            if let Some(Inline::Text(prev)) = merged.last_mut() {
                prev.push_str(&t);
                continue;
            }
            merged.push(Inline::Text(t));
        } else {
            merged.push(inline);
        }
    }
    *inlines = merged;
}

/// Parse a `---` front matter block into metadata. Unknown keys and comments
/// are ignored; quoted scalars are unescaped minimally.
fn parse_front_matter(raw: &str, meta: &mut Metadata) {
    let mut lines = raw.lines();
    if lines.next().map(str::trim) != Some("---") {
        return;
    }
    let body: Vec<&str> = lines.collect();
    // The closing delimiter is the last non-empty line.
    let end = body.iter().rposition(|l| l.trim() == "---");
    let Some(end) = end else { return };
    for line in &body[..end] {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = yaml_scalar(value.trim());
        match key.trim() {
            "title" => meta.title = Some(value),
            "author" => meta.author = Some(value),
            _ => {}
        }
    }
}

fn yaml_scalar(value: &str) -> String {
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        let inner = &value[1..value.len() - 1];
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            } else {
                out.push(c);
            }
        }
        out
    } else {
        value.to_owned()
    }
}

fn yaml_needs_quote(value: &str) -> bool {
    value.is_empty()
        || value.trim() != value
        || value.contains(['\n', ':', '#', '"', '\\', '\''])
        || value.starts_with([
            '-', '?', '>', '|', '*', '&', '!', '%', '@', '`', '{', '[', ',',
        ])
}

fn yaml_emit(value: &str) -> String {
    if yaml_needs_quote(value) {
        let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
        format!("\"{escaped}\"")
    } else {
        value.to_owned()
    }
}

impl Writer for MarkdownWriter {
    fn write(&self, doc: &Document, _fmt: Format) -> Result<Vec<u8>, WriteError> {
        let mut out = String::new();
        if doc.meta.title.is_some() || doc.meta.author.is_some() {
            out.push_str("---\n");
            if let Some(title) = &doc.meta.title {
                out.push_str(&format!("title: {}\n", yaml_emit(title)));
            }
            if let Some(author) = &doc.meta.author {
                out.push_str(&format!("author: {}\n", yaml_emit(author)));
            }
            out.push_str("---\n\n");
        }
        out.push_str(&blocks_to_md(&doc.blocks));
        if !out.ends_with('\n') {
            out.push('\n');
        }
        Ok(out.into_bytes())
    }
}

fn blocks_to_md(blocks: &[Block]) -> String {
    blocks
        .iter()
        .map(block_to_md)
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn block_to_md(block: &Block) -> String {
    match block {
        Block::Heading { level, inlines } => {
            let hashes = "#".repeat((*level as usize).clamp(1, 6));
            format!("{hashes} {}", inlines_to_md(inlines, true, false))
        }
        Block::Paragraph { inlines } => inlines_to_md(inlines, true, false),
        Block::List {
            ordered,
            start,
            items,
        } => items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let inner = blocks_to_md(item);
                let (marker, indent) = if *ordered {
                    let n = start + i as u64;
                    (format!("{n}. "), " ".repeat(n.to_string().len() + 2))
                } else {
                    ("- ".to_owned(), "  ".to_owned())
                };
                let mut lines = inner.lines();
                let mut out = match lines.next() {
                    Some(first) => format!("{marker}{first}"),
                    None => marker.trim_end().to_owned(),
                };
                for line in lines {
                    out.push('\n');
                    if line.is_empty() {
                        out.push_str(indent.trim_end());
                    } else {
                        out.push_str(&indent);
                        out.push_str(line);
                    }
                }
                out
            })
            .collect::<Vec<_>>()
            .join("\n"),
        Block::Table {
            header,
            rows,
            align,
        } => {
            let cell = |inlines: &[Inline]| {
                let text = inlines_to_md(inlines, true, true);
                text.replace('\n', " ")
            };
            let mut out = String::new();
            let cells: Vec<String> = header.iter().map(|c| cell(c)).collect();
            out.push_str(&format!("| {} |\n", cells.join(" | ")));
            let sep: Vec<String> = (0..header.len())
                .map(|i| match align.get(i).unwrap_or(&Align::None) {
                    Align::None => "---".to_owned(),
                    Align::Left => ":---".to_owned(),
                    Align::Center => ":---:".to_owned(),
                    Align::Right => "---:".to_owned(),
                })
                .collect();
            out.push_str(&format!("| {} |\n", sep.join(" | ")));
            for row in rows {
                let cells: Vec<String> = row.iter().map(|c| cell(c)).collect();
                out.push_str(&format!("| {} |\n", cells.join(" | ")));
            }
            out.pop();
            out
        }
        Block::CodeBlock { lang, text } => {
            let longest = text.split('`').map(str::len).max().unwrap_or(0);
            let fence = "`".repeat(longest.max(2) + 1);
            let info = lang.as_deref().unwrap_or("");
            let body = if text.ends_with('\n') {
                text.clone()
            } else {
                format!("{text}\n")
            };
            format!("{fence}{info}\n{body}{fence}")
        }
        Block::Image { src, alt } => {
            format!("![{}]({})", escape_text(alt, true, false), link_dest(src))
        }
        Block::Rule => "---".to_owned(),
        Block::Blockquote { blocks } => blocks_to_md(blocks)
            .lines()
            .map(|line| {
                if line.is_empty() {
                    ">".to_owned()
                } else {
                    format!("> {line}")
                }
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

fn inlines_to_md(inlines: &[Inline], at_line_start: bool, in_table: bool) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => {
                let line_start = at_line_start && out.is_empty();
                out.push_str(&escape_text(t, line_start, in_table));
            }
            Inline::Code(c) => out.push_str(&code_span(c)),
            Inline::Strong(inner) => {
                out.push_str("**");
                out.push_str(&inlines_to_md(inner, false, in_table));
                out.push_str("**");
            }
            Inline::Emph(inner) => {
                out.push('*');
                out.push_str(&inlines_to_md(inner, false, in_table));
                out.push('*');
            }
            Inline::Strikethrough(inner) => {
                out.push_str("~~");
                out.push_str(&inlines_to_md(inner, false, in_table));
                out.push_str("~~");
            }
            Inline::Link {
                dest,
                title,
                inlines,
            } => {
                out.push('[');
                out.push_str(&inlines_to_md(inlines, false, in_table));
                out.push_str("](");
                out.push_str(&link_dest(dest));
                if let Some(title) = title {
                    out.push_str(&format!(
                        " \"{}\"",
                        title.replace('\\', "\\\\").replace('"', "\\\"")
                    ));
                }
                out.push(')');
            }
        }
    }
    out
}

/// Escape a text run so it re-parses as literal text. `line_start` marks a
/// run written at the beginning of an output line (where `#`, `>`, `-`,
/// list ordinals, ... open new constructs); `in_table` adds `|`.
fn escape_text(text: &str, line_start: bool, in_table: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for (i, c) in text.char_indices() {
        let next = text[i + c.len_utf8()..].chars().next();
        let at_start = line_start && i == 0;
        // Digits directly before "." or ")" at line start form an ordered
        // list marker ("3. item"); escape the punctuation so the number
        // stays literal text.
        let digits_before =
            line_start && !text[..i].is_empty() && text[..i].chars().all(|c| c.is_ascii_digit());
        // A scheme followed by "//" opens a GFM autolink on reparse;
        // escape the colon so literal URL-shaped text stays text.
        let scheme = ["http://", "https://", "ftp://"].iter().any(|s| {
            let word = &s[..s.len() - 3]; // scheme without the "://"
            let before = &text[..i];
            before.len() >= word.len()
                && before.ends_with(word)
                && text[i + 1..].starts_with("//")
                && before[..before.len() - word.len()]
                    .chars()
                    .next_back()
                    .is_none_or(|c| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-')))
        });
        let escaped = match c {
            '\\' | '`' | '*' | '_' | '[' | ']' | '<' | '>' | '&' | '~' => true,
            '|' => in_table,
            '!' => next == Some('[') || next.is_none(),
            '#' => at_start || next == Some('#'), // "##" mid-text is still heading-ish
            '-' | '+' | '=' if at_start => true,
            '.' | ')' if digits_before => true,
            ':' if scheme => true,
            _ => false,
        };
        if escaped {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

fn code_span(content: &str) -> String {
    let longest = content.split('`').map(str::len).max().unwrap_or(0);
    let ticks = "`".repeat(longest + 1);
    let pad = if content.starts_with('`')
        || content.ends_with('`')
        || content.starts_with(' ')
        || content.ends_with(' ')
    {
        " "
    } else {
        ""
    };
    format!("{ticks}{pad}{content}{pad}{ticks}")
}

fn link_dest(dest: &str) -> String {
    if dest.contains([' ', '(', ')']) {
        format!("<{}>", dest.replace('>', "%3E"))
    } else {
        dest.to_owned()
    }
}

/// The shared Markdown fixture set (one source of truth for the md -> md
/// roundtrip here and the md <-> html cross-format roundtrips in
/// `formats/mod.rs`). Fixtures exercise the loss-prone seams: front matter
/// via title/author metadata, GFM table alignment, block-level `Image`,
/// RTL/Arabic with combining marks, and three-level nested lists.
#[cfg(test)]
pub(crate) fn md_fixtures() -> Vec<(&'static str, &'static str)> {
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
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Reader, Writer};

    fn parse_md(src: &str) -> Document {
        MarkdownReader
            .read(src.as_bytes(), Format::Markdown)
            .unwrap()
    }

    fn write_md(doc: &Document) -> String {
        String::from_utf8(MarkdownWriter.write(doc, Format::Markdown).unwrap()).unwrap()
    }

    /// The shared fixtures plus Markdown-only extras: every fixture parses
    /// and survives md -> md as a Document-value equality (AGENTS.md testing
    /// rule); md <-> html cross-format roundtrips live in `formats/mod.rs`
    /// against the same shared base.
    fn md_only_fixtures() -> Vec<(&'static str, &'static str)> {
        let mut fixtures = md_fixtures();
        fixtures.push((
            "literal metacharacters",
            "Use *stars* and _underscores_ literally: 2 * 3 and snake_case plus [brackets] stay text.\n",
        ));
        fixtures
    }

    #[test]
    fn md_roundtrips_are_document_equal() {
        for (name, src) in md_only_fixtures() {
            let once = parse_md(src);
            let twice = parse_md(&write_md(&once));
            assert_eq!(
                once,
                twice,
                "md->md roundtrip diverged: {name}\n--- written ---\n{}",
                write_md(&once)
            );
        }
    }

    #[test]
    fn front_matter_parses_into_metadata() {
        let doc =
            parse_md("---\ntitle: My: Title\nauthor: \"Quoted \\\"Author\\\"\"\n---\n\nbody\n");
        assert_eq!(doc.meta.title.as_deref(), Some("My: Title"));
        assert_eq!(doc.meta.author.as_deref(), Some("Quoted \"Author\""));
        assert_eq!(doc.blocks.len(), 1);
    }

    #[test]
    fn front_matter_roundtrips_tricky_values() {
        let doc = Document {
            meta: Metadata {
                title: Some("Has: colon and #hash".to_owned()),
                author: Some("  spaced  ".to_owned()),
            },
            blocks: vec![Block::Paragraph {
                inlines: vec![Inline::Text("body".to_owned())],
            }],
        };
        let back = parse_md(&write_md(&doc));
        assert_eq!(doc, back);
    }

    #[test]
    fn task_list_items_degrade_to_plain_text() {
        let doc = parse_md("- [ ] todo\n- [x] done\n");
        let Block::List { items, .. } = &doc.blocks[0] else {
            panic!("expected list");
        };
        let Block::Paragraph { inlines } = &items[0][0] else {
            panic!("expected paragraph");
        };
        assert!(matches!(&inlines[0], Inline::Text(t) if t.contains("[ ]")));
    }

    #[test]
    fn not_utf8_is_an_actionable_error() {
        let err = MarkdownReader
            .read(&[0xff, 0xfe], Format::Markdown)
            .unwrap_err();
        assert!(matches!(err, ReadError::NotUtf8));
        assert!(err.to_string().contains("UTF-8"));
    }

    #[test]
    fn escapes_prevent_structure_on_reparse() {
        let tricky = vec![
            "# not a heading",
            "- not a list",
            "3. not a list either",
            "> not a quote",
            "a [link](http://x) shaped string",
            "**not strong** but with stars",
        ];
        for text in tricky {
            let doc = Document {
                meta: Metadata::default(),
                blocks: vec![Block::Paragraph {
                    inlines: vec![Inline::Text(text.to_owned())],
                }],
            };
            let written = write_md(&doc);
            assert_eq!(
                parse_md(&written),
                doc,
                "literal text shifted meaning: {text:?} -> {written:?}"
            );
        }
    }

    #[test]
    fn writer_emits_trailing_newline() {
        let doc = Document {
            meta: Metadata::default(),
            blocks: vec![Block::Paragraph {
                inlines: vec![Inline::Text("x".to_owned())],
            }],
        };
        assert!(write_md(&doc).ends_with('\n'));
    }
}
