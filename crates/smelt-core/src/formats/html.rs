//! HTML converter: a scraper (html5ever) DOM walk on read, a hand-rolled
//! serializer on write.
//!
//! Reader: `Html::parse_document` repairs the input first (FORMATS.md lists
//! that as lossy), then block children of `body` are walked into the IDM.
//! Whitespace collapses per HTML rendering semantics; `script`/`style` and
//! other non-content elements are dropped, along with classes and inline
//! styles -- the IDM carries structure, not presentation. Document `title`
//! and `<meta name="author">` map to IDM metadata.
//!
//! Writer: IDM -> a complete HTML5 document (`<!DOCTYPE>`, head with
//! title/author, body). Block `Image` becomes a bare `<img>` (flow content);
//! table column alignment becomes `align` attributes, both so that reading
//! our own output back restores the exact `Document`.
//!
//! [`fragment_blocks`] exposes the same block walker for markdown raw-HTML
//! blocks (`formats::markdown`); metadata extraction is reader-only.

use scraper::{ElementRef, Html, node::Node};

use smelt_detect::Format;

use crate::error::{ReadError, WriteError};
use crate::idm::{Align, Block, Document, Inline, Metadata};
use crate::{Reader, Writer};

use super::{collapse_ws, is_ws, merge_text};

/// The html reader (scraper DOM walk -> IDM).
pub struct HtmlReader;

/// The html writer (IDM -> html).
pub struct HtmlWriter;

impl Reader for HtmlReader {
    fn read(&self, input: &[u8], _fmt: Format) -> Result<Document, ReadError> {
        let text = std::str::from_utf8(input).map_err(|_| ReadError::NotUtf8)?;
        let dom = Html::parse_document(text);
        let root = dom.root_element();
        let mut meta = Metadata::default();
        read_meta(root, &mut meta);
        let mut blocks = Vec::new();
        match find_body(root) {
            Some(body) => walk_blocks(body, &mut blocks),
            // parse_document always synthesises a body; the fallback keeps
            // us honest if that ever changes.
            None => walk_blocks(root, &mut blocks),
        }
        Ok(Document { meta, blocks })
    }
}

impl Writer for HtmlWriter {
    fn write(&self, doc: &Document, _fmt: Format) -> Result<Vec<u8>, WriteError> {
        let mut out = String::with_capacity(1024);
        out.push_str("<!DOCTYPE html>\n<html>\n<head>\n");
        if let Some(title) = &doc.meta.title {
            out.push_str("<title>");
            out.push_str(&esc(title));
            out.push_str("</title>\n");
        }
        if let Some(author) = &doc.meta.author {
            out.push_str("<meta name=\"author\" content=\"");
            out.push_str(&esc(author));
            out.push_str("\">\n");
        }
        out.push_str("</head>\n<body>\n");
        for block in &doc.blocks {
            write_block(block, &mut out);
        }
        out.push_str("</body>\n</html>\n");
        Ok(out.into_bytes())
    }
}

/// Parse a raw-HTML fragment (a markdown HTML block) into top-level blocks.
/// Used by `formats::markdown`; metadata is not extracted here.
pub(crate) fn fragment_blocks(html: &str) -> Vec<Block> {
    let dom = Html::parse_document(html);
    let root = dom.root_element();
    let mut out = Vec::new();
    match find_body(root) {
        Some(body) => walk_blocks(body, &mut out),
        None => walk_blocks(root, &mut out),
    }
    out
}

// ---------------------------------------------------------------------------
// Reader helpers
// ---------------------------------------------------------------------------

fn find_body(root: ElementRef) -> Option<ElementRef> {
    root.child_elements().find(|e| e.value().name() == "body")
}

/// Extract `<title>` and `<meta name="author">` from the head.
fn read_meta(root: ElementRef, meta: &mut Metadata) {
    let Some(head) = root.child_elements().find(|e| e.value().name() == "head") else {
        return;
    };
    for child in head.child_elements() {
        match child.value().name() {
            "title" => {
                let title = child.text().collect::<String>();
                let title = title.trim();
                if !title.is_empty() {
                    meta.title = Some(title.to_owned());
                }
            }
            "meta" => {
                let is_author = child
                    .value()
                    .attr("name")
                    .is_some_and(|n| n.eq_ignore_ascii_case("author"));
                if is_author
                    && let Some(author) = child.value().attr("content")
                    && !author.trim().is_empty()
                {
                    meta.author = Some(author.trim().to_owned());
                }
            }
            _ => {}
        }
    }
}

/// Elements whose content is dropped outright: executable, embedded, or
/// head-only content that carries no document structure.
fn is_dropped(name: &str) -> bool {
    matches!(
        name,
        "script"
            | "style"
            | "noscript"
            | "template"
            | "head"
            | "title"
            | "meta"
            | "link"
            | "base"
            | "iframe"
            | "svg"
            | "math"
            | "object"
            | "embed"
            | "applet"
            | "canvas"
            | "video"
            | "audio"
            | "source"
            | "track"
            | "param"
            | "map"
            | "area"
            | "col"
            | "colgroup"
    )
}

/// Inline-level elements at block position (a stray `<a>` or `<span>`
/// between paragraphs) contribute their inlines to the pending run.
fn is_inline(name: &str) -> bool {
    matches!(
        name,
        "strong"
            | "b"
            | "em"
            | "i"
            | "del"
            | "s"
            | "strike"
            | "ins"
            | "code"
            | "a"
            | "br"
            | "wbr"
            | "span"
            | "q"
            | "small"
            | "u"
            | "mark"
            | "time"
            | "sub"
            | "sup"
            | "kbd"
            | "samp"
            | "var"
            | "cite"
            | "abbr"
            | "bdi"
            | "bdo"
            | "ruby"
            | "rt"
            | "rp"
            | "dfn"
            | "label"
            | "output"
            | "big"
            | "font"
            | "tt"
    )
}

/// Walk block-level children of `el` into `out`, buffering loose text into
/// paragraphs. Promoted images (from inline positions) land after the
/// paragraph that contained them.
fn walk_blocks(el: ElementRef, out: &mut Vec<Block>) {
    let mut pending: Vec<Inline> = Vec::new();
    let mut promoted: Vec<Block> = Vec::new();
    for child in el.children() {
        if let Node::Text(t) = child.value() {
            let collapsed = collapse_ws(&t.text);
            if !collapsed.is_empty() {
                pending.push(Inline::Text(collapsed));
            }
            continue;
        }
        if !child.value().is_element() {
            continue;
        }
        let Some(child_el) = ElementRef::wrap(child) else {
            continue;
        };
        let name = child_el.value().name();
        match name {
            _ if is_dropped(name) => continue,
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                flush(&mut pending, &mut promoted, out);
                let level: u8 = name[1..].parse().unwrap_or(1);
                let (inlines, mut images) = collect_inlines(child_el);
                out.push(Block::Heading { level, inlines });
                out.append(&mut images);
            }
            "p" => {
                flush(&mut pending, &mut promoted, out);
                paragraph_blocks(child_el, out);
            }
            "ul" | "ol" => {
                flush(&mut pending, &mut promoted, out);
                out.push(list_block(child_el));
            }
            "blockquote" => {
                flush(&mut pending, &mut promoted, out);
                let mut inner = Vec::new();
                walk_blocks(child_el, &mut inner);
                out.push(Block::Blockquote { blocks: inner });
            }
            "pre" => {
                flush(&mut pending, &mut promoted, out);
                out.push(code_block(child_el));
            }
            "table" => {
                flush(&mut pending, &mut promoted, out);
                if let Some(table) = table_block(child_el) {
                    out.push(table);
                }
            }
            "img" => {
                flush(&mut pending, &mut promoted, out);
                out.push(image_block(child_el));
            }
            "hr" => {
                flush(&mut pending, &mut promoted, out);
                out.push(Block::Rule);
            }
            _ if is_inline(name) => {
                push_inline_element(child_el, &mut pending, &mut promoted);
            }
            // Structural containers (div, section, article, figure, dl, ...)
            // and unknown elements: transparent; recurse.
            _ => {
                flush(&mut pending, &mut promoted, out);
                let mut nested = Vec::new();
                walk_blocks(child_el, &mut nested);
                out.append(&mut nested);
            }
        }
    }
    flush(&mut pending, &mut promoted, out);
}

/// Emit the buffered text as a paragraph, then any promoted images.
fn flush(pending: &mut Vec<Inline>, promoted: &mut Vec<Block>, out: &mut Vec<Block>) {
    finish_inlines(pending);
    if !pending.is_empty() {
        out.push(Block::Paragraph {
            inlines: std::mem::take(pending),
        });
    }
    out.append(promoted);
}

/// Merge, edge-trim, and drop empties from a finished inline run. Only the
/// outer edges of the run trim: leading whitespace falls off the first text
/// run, trailing whitespace off the last, but interior boundaries (the space
/// before `<strong>` in `hi <strong>there</strong>`) keep their collapsed
/// single space.
fn finish_inlines(inlines: &mut Vec<Inline>) {
    merge_text(inlines);
    if let Some(Inline::Text(t)) = inlines.first_mut() {
        *t = t.trim_start_matches(is_ws).to_owned();
    }
    if let Some(Inline::Text(t)) = inlines.last_mut() {
        *t = t.trim_end_matches(is_ws).to_owned();
    }
    inlines.retain(|i| !matches!(i, Inline::Text(t) if t.is_empty()));
}

/// `<p>` content. A direct `<img>` child splits the paragraph exactly the
/// way the markdown reader promotes inline images: text before, image,
/// text after.
fn paragraph_blocks(el: ElementRef, out: &mut Vec<Block>) {
    let mut current: Vec<Inline> = Vec::new();
    let mut images: Vec<Block> = Vec::new();
    for child in el.children() {
        if let Node::Text(t) = child.value() {
            let collapsed = collapse_ws(&t.text);
            if !collapsed.is_empty() {
                current.push(Inline::Text(collapsed));
            }
            continue;
        }
        if !child.value().is_element() {
            continue;
        }
        let Some(child_el) = ElementRef::wrap(child) else {
            continue;
        };
        if child_el.value().name() == "img" {
            finish_inlines(&mut current);
            if !current.is_empty() {
                out.push(Block::Paragraph {
                    inlines: std::mem::take(&mut current),
                });
            }
            out.push(image_block(child_el));
            continue;
        }
        push_inline_element(child_el, &mut current, &mut images);
    }
    finish_inlines(&mut current);
    if !current.is_empty() {
        out.push(Block::Paragraph { inlines: current });
    }
    out.append(&mut images);
}

/// Collect the inline content of an element (heading, cell, wrapper...).
/// Returns promoted images separately; the caller decides where they land.
fn collect_inlines(el: ElementRef) -> (Vec<Inline>, Vec<Block>) {
    let mut inlines = Vec::new();
    let mut images = Vec::new();
    walk_inlines(el, &mut inlines, &mut images);
    finish_inlines(&mut inlines);
    (inlines, images)
}

fn walk_inlines(el: ElementRef, out: &mut Vec<Inline>, images: &mut Vec<Block>) {
    for child in el.children() {
        if let Node::Text(t) = child.value() {
            let collapsed = collapse_ws(&t.text);
            if !collapsed.is_empty() {
                out.push(Inline::Text(collapsed));
            }
            continue;
        }
        if !child.value().is_element() {
            continue;
        }
        let Some(child_el) = ElementRef::wrap(child) else {
            continue;
        };
        push_inline_element(child_el, out, images);
    }
}

/// Map one inline element onto the IDM. Unknown inline-ish elements are
/// transparent (children flattened); empty wrappers drop, matching the
/// markdown reader.
fn push_inline_element(el: ElementRef, out: &mut Vec<Inline>, images: &mut Vec<Block>) {
    let name = el.value().name();
    match name {
        "strong" | "b" => wrap(el, out, images, Inline::Strong),
        "em" | "i" => wrap(el, out, images, Inline::Emph),
        "del" | "s" | "strike" => wrap(el, out, images, Inline::Strikethrough),
        "code" => {
            let text = el.text().collect::<String>();
            if !text.is_empty() {
                out.push(Inline::Code(text));
            }
        }
        "a" => {
            let (inner, mut inner_images) = collect_inlines(el);
            images.append(&mut inner_images);
            if !inner.is_empty() {
                out.push(Inline::Link {
                    dest: el.value().attr("href").unwrap_or("").to_owned(),
                    title: el
                        .value()
                        .attr("title")
                        .filter(|t| !t.is_empty())
                        .map(str::to_owned),
                    inlines: inner,
                });
            }
        }
        "br" | "wbr" => out.push(Inline::Text(" ".to_owned())),
        "img" => images.push(image_block(el)),
        // Transparent inline containers and unknown inline-ish elements.
        _ => walk_inlines(el, out, images),
    }
}

fn wrap(
    el: ElementRef,
    out: &mut Vec<Inline>,
    images: &mut Vec<Block>,
    f: fn(Vec<Inline>) -> Inline,
) {
    let (inner, mut inner_images) = collect_inlines(el);
    images.append(&mut inner_images);
    if !inner.is_empty() {
        out.push(f(inner));
    }
}

fn image_block(el: ElementRef) -> Block {
    Block::Image {
        src: el.value().attr("src").unwrap_or("").to_owned(),
        alt: el.value().attr("alt").unwrap_or("").to_owned(),
    }
}

/// `<pre>` -> CodeBlock. Language comes from a `language-`/`lang-` class on
/// the inner `<code>`; other classes are presentation and drop.
fn code_block(el: ElementRef) -> Block {
    let code = el.child_elements().find(|c| c.value().name() == "code");
    let (lang, mut text) = match code {
        Some(code) => (language_class(code), code.text().collect::<String>()),
        None => (language_class(el), el.text().collect::<String>()),
    };
    // Normalise the trailing newline the way the markdown reader does so
    // that `Document` values survive md -> html -> md unchanged.
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
    Block::CodeBlock { lang, text }
}

fn language_class(el: ElementRef) -> Option<String> {
    let class = el.value().attr("class")?;
    class.split_whitespace().find_map(|token| {
        token
            .strip_prefix("language-")
            .or_else(|| token.strip_prefix("lang-"))
            .filter(|lang| !lang.is_empty())
            .map(str::to_owned)
    })
}

/// Collect `<tr>` descendants of a table without descending into nested
/// tables.
fn collect_rows<'a>(el: ElementRef<'a>, out: &mut Vec<ElementRef<'a>>) {
    for child in el.child_elements() {
        match child.value().name() {
            "tr" => out.push(child),
            "thead" | "tbody" | "tfoot" => collect_rows(child, out),
            _ => {}
        }
    }
}

fn cell_align(el: ElementRef) -> Align {
    if let Some(a) = el.value().attr("align") {
        match a.trim().to_ascii_lowercase().as_str() {
            "left" => return Align::Left,
            "center" | "middle" => return Align::Center,
            "right" => return Align::Right,
            _ => {}
        }
    }
    if let Some(style) = el.value().attr("style")
        && let Some(pos) = style.find("text-align")
    {
        let rest = &style[pos + "text-align".len()..];
        if let Some(value) = rest.strip_prefix(':') {
            let value = value.split(';').next().unwrap_or("").trim();
            match value.to_ascii_lowercase().as_str() {
                "left" => return Align::Left,
                "center" => return Align::Center,
                "right" => return Align::Right,
                _ => {}
            }
        }
    }
    Align::None
}

fn cells_of(tr: ElementRef) -> Vec<Vec<Inline>> {
    tr.child_elements()
        .filter(|c| matches!(c.value().name(), "th" | "td"))
        .map(|c| collect_inlines(c).0)
        .collect()
}

/// `table` -> Table. The first row is the header (markdown needs one); all
/// rows are padded or truncated to the header width to keep the IDM
/// invariant `row.len() == header.len()` on malformed input.
fn table_block(el: ElementRef) -> Option<Block> {
    let mut rows = Vec::new();
    collect_rows(el, &mut rows);
    let mut iter = rows.into_iter();
    let header_row = iter.next()?;
    let header = cells_of(header_row);
    if header.is_empty() {
        return None;
    }
    let align: Vec<Align> = header_row
        .child_elements()
        .filter(|c| matches!(c.value().name(), "th" | "td"))
        .map(cell_align)
        .collect();
    let mut body = Vec::new();
    for tr in iter {
        let mut row = cells_of(tr);
        row.resize(header.len(), Vec::new());
        body.push(row);
    }
    Some(Block::Table {
        header,
        rows: body,
        align,
    })
}

fn list_block(el: ElementRef) -> Block {
    let ordered = el.value().name() == "ol";
    let start = if ordered {
        el.value()
            .attr("start")
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(1)
            .max(1)
    } else {
        0
    };
    let items = el
        .child_elements()
        .filter(|c| c.value().name() == "li")
        .map(|li| {
            let mut blocks = Vec::new();
            walk_blocks(li, &mut blocks);
            blocks
        })
        .collect();
    Block::List {
        ordered,
        start,
        items,
    }
}

// ---------------------------------------------------------------------------
// Writer helpers
// ---------------------------------------------------------------------------

fn esc(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

fn write_block(block: &Block, out: &mut String) {
    match block {
        Block::Heading { level, inlines } => {
            let level = (*level).clamp(1, 6);
            out.push_str("<h");
            out.push_str(&level.to_string());
            out.push('>');
            out.push_str(&inlines_html(inlines));
            out.push_str("</h");
            out.push_str(&level.to_string());
            out.push_str(">\n");
        }
        Block::Paragraph { inlines } => {
            if inlines.is_empty() {
                return;
            }
            out.push_str("<p>");
            out.push_str(&inlines_html(inlines));
            out.push_str("</p>\n");
        }
        Block::List {
            ordered,
            start,
            items,
        } => {
            if *ordered && *start > 1 {
                out.push_str("<ol start=\"");
                out.push_str(&start.to_string());
                out.push_str("\">\n");
            } else if *ordered {
                out.push_str("<ol>\n");
            } else {
                out.push_str("<ul>\n");
            }
            for item in items {
                out.push_str("<li>");
                write_item(item, out);
                out.push_str("</li>\n");
            }
            out.push_str(if *ordered { "</ol>\n" } else { "</ul>\n" });
        }
        Block::Table {
            header,
            rows,
            align,
        } => {
            out.push_str("<table>\n<thead>\n<tr>");
            for (i, cell) in header.iter().enumerate() {
                out.push_str("<th");
                if let Some(a) = align.get(i)
                    && let Some(value) = align_attr(a)
                {
                    out.push_str(" align=\"");
                    out.push_str(value);
                    out.push('"');
                }
                out.push('>');
                out.push_str(&inlines_html(cell));
                out.push_str("</th>");
            }
            out.push_str("</tr>\n</thead>\n<tbody>\n");
            for row in rows {
                out.push_str("<tr>");
                for cell in row {
                    out.push_str("<td>");
                    out.push_str(&inlines_html(cell));
                    out.push_str("</td>");
                }
                out.push_str("</tr>\n");
            }
            out.push_str("</tbody>\n</table>\n");
        }
        Block::CodeBlock { lang, text } => {
            out.push_str("<pre><code");
            if let Some(lang) = lang
                && !lang.is_empty()
            {
                out.push_str(" class=\"language-");
                out.push_str(&esc(lang));
                out.push('"');
            }
            out.push('>');
            out.push_str(&esc(text));
            out.push_str("</code></pre>\n");
        }
        Block::Image { src, alt } => {
            out.push_str("<img src=\"");
            out.push_str(&esc(src));
            out.push_str("\" alt=\"");
            out.push_str(&esc(alt));
            out.push_str("\">\n");
        }
        Block::Rule => out.push_str("<hr>\n"),
        Block::Blockquote { blocks } => {
            out.push_str("<blockquote>\n");
            for block in blocks {
                write_block(block, out);
            }
            out.push_str("</blockquote>\n");
        }
    }
}

fn align_attr(align: &Align) -> Option<&'static str> {
    match align {
        Align::Left => Some("left"),
        Align::Center => Some("center"),
        Align::Right => Some("right"),
        Align::None => None,
    }
}

/// `<li>` content: a leading paragraph becomes bare inline content (so the
/// usual `- item` shape serialises as `<li>item</li>`); anything after it is
/// written as full block markup inside the element.
fn write_item(item: &[Block], out: &mut String) {
    let mut wrote_paragraph = false;
    for block in item {
        match block {
            Block::Paragraph { inlines } if inlines.is_empty() => continue,
            Block::Paragraph { inlines } => {
                if wrote_paragraph {
                    write_block(block, out);
                } else {
                    out.push_str(&inlines_html(inlines));
                    wrote_paragraph = true;
                }
            }
            other => write_block(other, out),
        }
    }
}

fn inlines_html(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => out.push_str(&esc(t)),
            Inline::Code(c) => {
                out.push_str("<code>");
                out.push_str(&esc(c));
                out.push_str("</code>");
            }
            Inline::Strong(inner) => {
                out.push_str("<strong>");
                out.push_str(&inlines_html(inner));
                out.push_str("</strong>");
            }
            Inline::Emph(inner) => {
                out.push_str("<em>");
                out.push_str(&inlines_html(inner));
                out.push_str("</em>");
            }
            Inline::Strikethrough(inner) => {
                out.push_str("<del>");
                out.push_str(&inlines_html(inner));
                out.push_str("</del>");
            }
            Inline::Link {
                dest,
                title,
                inlines,
            } => {
                out.push_str("<a href=\"");
                out.push_str(&esc(dest));
                out.push('"');
                if let Some(title) = title {
                    out.push_str(" title=\"");
                    out.push_str(&esc(title));
                    out.push('"');
                }
                out.push('>');
                out.push_str(&inlines_html(inlines));
                out.push_str("</a>");
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

    fn parse(src: &str) -> Document {
        HtmlReader.read(src.as_bytes(), Format::Html).unwrap()
    }

    #[test]
    fn head_title_and_author_reach_metadata() {
        let doc = parse(
            "<!DOCTYPE html>\n<html>\n<head>\n<title> Doc Title \n</title>\n\
             <meta name=\"author\" content=\"A. Author\">\n</head>\n<body>\n\
             <p>x</p>\n</body>\n</html>\n",
        );
        assert_eq!(doc.meta.title.as_deref(), Some("Doc Title"));
        assert_eq!(doc.meta.author.as_deref(), Some("A. Author"));
    }

    #[test]
    fn script_and_style_are_dropped() {
        let doc = parse("<body><script>var x = 1;</script><p>kept</p><style>p{}</style></body>");
        assert_eq!(
            doc.blocks,
            vec![Block::Paragraph {
                inlines: vec![Inline::Text("kept".to_owned())]
            }]
        );
    }

    #[test]
    fn language_class_maps_to_lang_and_other_classes_drop() {
        let doc = parse(
            "<body><pre><code class=\"language-rust highlight\">fn x() {}</code></pre></body>",
        );
        assert_eq!(
            doc.blocks,
            vec![Block::CodeBlock {
                lang: Some("rust".to_owned()),
                text: "fn x() {}\n".to_owned()
            }]
        );
    }

    #[test]
    fn code_text_gains_a_trailing_newline() {
        let doc = parse("<body><pre><code>no newline</code></pre></body>");
        assert_eq!(
            doc.blocks,
            vec![Block::CodeBlock {
                lang: None,
                text: "no newline\n".to_owned()
            }]
        );
    }

    #[test]
    fn table_rows_pad_to_header_width() {
        let doc = parse(
            "<body><table><tr><th>a</th><th>b</th></tr>\
             <tr><td>1</td></tr></table></body>",
        );
        match &doc.blocks[..] {
            [
                Block::Table {
                    header,
                    rows,
                    align,
                },
            ] => {
                assert_eq!(header.len(), 2);
                assert_eq!(rows.len(), 1);
                assert_eq!(rows[0].len(), 2);
                assert_eq!(*align, vec![Align::None, Align::None]);
            }
            other => panic!("expected one table, got {other:?}"),
        }
    }

    #[test]
    fn ordered_start_clamps_to_one() {
        let doc = parse("<body><ol start=\"0\"><li>x</li></ol></body>");
        assert_eq!(
            doc.blocks,
            vec![Block::List {
                ordered: true,
                start: 1,
                items: vec![vec![Block::Paragraph {
                    inlines: vec![Inline::Text("x".to_owned())]
                }]]
            }]
        );
    }

    #[test]
    fn fragment_blocks_parses_without_body_wrapper() {
        let blocks = fragment_blocks("<p>hi <b>there</b></p>");
        assert_eq!(
            blocks,
            vec![Block::Paragraph {
                inlines: vec![
                    Inline::Text("hi ".to_owned()),
                    Inline::Strong(vec![Inline::Text("there".to_owned())])
                ]
            }]
        );
    }
}
