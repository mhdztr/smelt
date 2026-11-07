//! Native RTF (Rich Text Format) reader and writer.
//! Zero external runtime or dependencies required.

use smelt_detect::Format;

use crate::error::{ReadError, WriteError};
use crate::idm::{Align, Block, Document, Inline, Metadata};
use crate::{Reader, Writer};

/// The native RTF reader.
pub struct RtfReader;

/// The native RTF writer.
pub struct RtfWriter;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Destination {
    Normal,
    Ignored,
    InfoTitle,
    InfoAuthor,
}

#[derive(Debug, Clone)]
struct RtfState {
    bold: bool,
    italic: bool,
    strike: bool,
    font_size: u32,
    dest: Destination,
}

impl Default for RtfState {
    fn default() -> Self {
        Self {
            bold: false,
            italic: false,
            strike: false,
            font_size: 24, // 12pt default
            dest: Destination::Normal,
        }
    }
}

impl Reader for RtfReader {
    fn read(&self, input: &[u8], _fmt: Format) -> Result<Document, ReadError> {
        // Find {\rtf magic
        let text = String::from_utf8_lossy(input);
        let trimmed = text.trim_start();
        if !trimmed.starts_with("{\\rtf") {
            return Err(ReadError::Parse {
                format: "rtf".to_string(),
                message: "input does not begin with RTF magic '{\\rtf'".to_string(),
            });
        }

        let mut parser = RtfParser::new(&text);
        parser.parse()
    }
}

struct RtfParser<'a> {
    chars: std::str::Chars<'a>,
    peeked: Option<char>,
    state_stack: Vec<RtfState>,
    current_state: RtfState,
    paragraph_heading: Option<u8>,
    paragraph_max_fs: u32,
    meta_title: Option<String>,
    meta_author: Option<String>,
    blocks: Vec<Block>,
    current_inlines: Vec<Inline>,
    current_cell_inlines: Vec<Inline>,
    table_row: Vec<Vec<Inline>>,
    in_table: bool,
}

impl<'a> RtfParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            chars: input.chars(),
            peeked: None,
            state_stack: Vec::new(),
            current_state: RtfState::default(),
            paragraph_heading: None,
            paragraph_max_fs: 24,
            meta_title: None,
            meta_author: None,
            blocks: Vec::new(),
            current_inlines: Vec::new(),
            current_cell_inlines: Vec::new(),
            table_row: Vec::new(),
            in_table: false,
        }
    }

    fn next_char(&mut self) -> Option<char> {
        if let Some(c) = self.peeked.take() {
            Some(c)
        } else {
            self.chars.next()
        }
    }

    fn peek_char(&mut self) -> Option<char> {
        if self.peeked.is_none() {
            self.peeked = self.chars.next();
        }
        self.peeked
    }

    fn parse(&mut self) -> Result<Document, ReadError> {
        let mut text_buf = String::new();

        while let Some(c) = self.next_char() {
            match c {
                '{' => {
                    self.flush_text(&mut text_buf);
                    self.state_stack.push(self.current_state.clone());
                }
                '}' => {
                    self.flush_text(&mut text_buf);
                    if let Some(prev) = self.state_stack.pop() {
                        self.current_state = prev;
                    }
                }
                '\\' => {
                    self.flush_text(&mut text_buf);
                    self.parse_control(&mut text_buf)?;
                }
                '\r' | '\n' => {
                    // RTF ignores raw newlines
                }
                _ => {
                    if self.current_state.dest != Destination::Ignored {
                        text_buf.push(c);
                    }
                }
            }
        }
        self.flush_text(&mut text_buf);
        self.flush_paragraph();

        let doc = Document {
            meta: Metadata {
                title: self.meta_title.clone(),
                author: self.meta_author.clone(),
            },
            blocks: std::mem::take(&mut self.blocks),
        };
        Ok(doc)
    }

    fn flush_text(&mut self, buf: &mut String) {
        if buf.is_empty() {
            return;
        }
        let text = std::mem::take(buf);
        match self.current_state.dest {
            Destination::Ignored => {}
            Destination::InfoTitle => {
                let existing = self.meta_title.get_or_insert_with(String::new);
                existing.push_str(&text);
            }
            Destination::InfoAuthor => {
                let existing = self.meta_author.get_or_insert_with(String::new);
                existing.push_str(&text);
            }
            Destination::Normal => {
                let mut inline = Inline::Text(text);
                if self.current_state.strike {
                    inline = Inline::Strikethrough(vec![inline]);
                }
                if self.current_state.italic {
                    inline = Inline::Emph(vec![inline]);
                }
                if self.current_state.bold {
                    inline = Inline::Strong(vec![inline]);
                }
                if self.in_table {
                    self.current_cell_inlines.push(inline);
                } else {
                    self.current_inlines.push(inline);
                }
            }
        }
    }

    fn flush_paragraph(&mut self) {
        if self.in_table {
            return;
        }
        if self.current_inlines.is_empty() {
            return;
        }
        let inlines = std::mem::take(&mut self.current_inlines);
        let normalized = normalize_inlines(inlines);
        if normalized.is_empty() {
            return;
        }

        // Determine if heading by heading style or font size
        // \fs44 is 22pt, \fs36 is 18pt, \fs28 is 14pt
        let heading_level = if let Some(lvl) = self.paragraph_heading {
            Some(lvl)
        } else if self.paragraph_max_fs >= 44 {
            Some(1)
        } else if self.paragraph_max_fs >= 36 {
            Some(2)
        } else if self.paragraph_max_fs >= 28 {
            Some(3)
        } else {
            None
        };

        let block = if let Some(lvl) = heading_level {
            Block::Heading {
                level: lvl.clamp(1, 6),
                inlines: normalized,
            }
        } else {
            Block::Paragraph {
                inlines: normalized,
            }
        };
        self.blocks.push(block);
        self.paragraph_heading = None;
        self.paragraph_max_fs = self.current_state.font_size;
    }

    fn parse_control(&mut self, buf: &mut String) -> Result<(), ReadError> {
        let Some(first) = self.next_char() else {
            return Ok(());
        };

        // Check for control symbols: \{, \}, \\, \~, \-, \_
        if first == '{' || first == '}' || first == '\\' {
            if self.current_state.dest != Destination::Ignored {
                buf.push(first);
            }
            return Ok(());
        }
        if first == '~' {
            if self.current_state.dest != Destination::Ignored {
                buf.push('\u{00A0}'); // Non-breaking space
            }
            return Ok(());
        }
        if first == '-' || first == '_' {
            return Ok(());
        }
        if first == '\'' {
            // Hex byte \'xx
            let mut hex = String::new();
            if let Some(h1) = self.next_char() {
                hex.push(h1);
            }
            if let Some(h2) = self.next_char() {
                hex.push(h2);
            }
            if let Ok(byte) = u8::from_str_radix(&hex, 16)
                && self.current_state.dest != Destination::Ignored
            {
                buf.push(byte as char);
            }
            return Ok(());
        }
        if first == '*' {
            // Ignored destination: \*...
            self.current_state.dest = Destination::Ignored;
            return Ok(());
        }

        // Control word: letters followed by optional signed integer
        let mut word = String::new();
        word.push(first);
        while let Some(c) = self.peek_char() {
            if c.is_ascii_alphabetic() {
                word.push(c);
                self.next_char();
            } else {
                break;
            }
        }

        let mut param: Option<i32> = None;
        let mut param_str = String::new();
        if let Some(c) = self.peek_char()
            && (c == '-' || c.is_ascii_digit())
        {
            param_str.push(c);
            self.next_char();
            while let Some(d) = self.peek_char() {
                if d.is_ascii_digit() {
                    param_str.push(d);
                    self.next_char();
                } else {
                    break;
                }
            }
            param = param_str.parse::<i32>().ok();
        }

        // Delimiter space is consumed
        if self.peek_char() == Some(' ') {
            self.next_char();
        }

        // Handle word
        match word.as_str() {
            "fonttbl" | "colortbl" | "stylesheet" | "filetable" | "revtbl" | "header"
            | "footer" | "pict" => {
                self.current_state.dest = Destination::Ignored;
            }
            "title" => {
                self.current_state.dest = Destination::InfoTitle;
            }
            "author" => {
                self.current_state.dest = Destination::InfoAuthor;
            }
            "b" => {
                self.current_state.bold = param != Some(0);
            }
            "i" => {
                self.current_state.italic = param != Some(0);
            }
            "strike" => {
                self.current_state.strike = param != Some(0);
            }
            "s" => {
                if let Some(num) = param
                    && (1..=6).contains(&num)
                {
                    self.paragraph_heading = Some(num as u8);
                }
            }
            "pard" => {
                self.paragraph_heading = None;
                self.paragraph_max_fs = self.current_state.font_size;
            }
            "fs" => {
                if let Some(size) = param {
                    let s = size.max(1) as u32;
                    self.current_state.font_size = s;
                    self.paragraph_max_fs = self.paragraph_max_fs.max(s);
                }
            }
            "par" => {
                self.flush_paragraph();
            }
            "line" => {
                buf.push('\n');
            }
            "tab" => {
                buf.push('\t');
            }
            "bullet" => {
                buf.push('*');
            }
            "u" => {
                if let Some(codepoint) = param {
                    let u16_val = if codepoint < 0 {
                        (codepoint + 65536) as u16
                    } else {
                        codepoint as u16
                    };
                    if let Some(ch) = char::from_u32(u16_val as u32) {
                        buf.push(ch);
                    }
                    // Skip replacement character (usually '?')
                    if self.peek_char().is_some() {
                        self.next_char();
                    }
                }
            }
            "trowd" => {
                self.in_table = true;
                self.table_row.clear();
                self.current_cell_inlines.clear();
            }
            "cell" => {
                let cell_inlines = std::mem::take(&mut self.current_cell_inlines);
                self.table_row.push(normalize_inlines(cell_inlines));
            }
            "row" => {
                let row_cells = std::mem::take(&mut self.table_row);
                if !row_cells.is_empty() {
                    // Check if continuing an existing table or starting a new table
                    if let Some(Block::Table { rows, header, .. }) = self.blocks.last_mut() {
                        if row_cells.len() == header.len() {
                            rows.push(row_cells);
                        } else {
                            // Column count mismatch: start new table
                            let cols = row_cells.len();
                            self.blocks.push(Block::Table {
                                header: row_cells,
                                rows: Vec::new(),
                                align: vec![Align::None; cols],
                            });
                        }
                    } else {
                        let cols = row_cells.len();
                        self.blocks.push(Block::Table {
                            header: row_cells,
                            rows: Vec::new(),
                            align: vec![Align::None; cols],
                        });
                    }
                }
                self.in_table = false;
            }
            _ => {}
        }

        Ok(())
    }
}

impl Writer for RtfWriter {
    fn write(&self, doc: &Document, _fmt: Format) -> Result<Vec<u8>, WriteError> {
        let mut out = String::new();

        // RTF Header
        out.push_str("{\\rtf1\\ansi\\deff0\n");
        out.push_str("{\\fonttbl{\\f0 Arial;}{\\f1 Courier New;}}\n");

        // Document info / metadata
        if doc.meta.title.is_some() || doc.meta.author.is_some() {
            out.push_str("{\\info");
            if let Some(ref title) = doc.meta.title {
                out.push_str("{\\title ");
                escape_rtf_str(&mut out, title);
                out.push('}');
            }
            if let Some(ref author) = doc.meta.author {
                out.push_str("{\\author ");
                escape_rtf_str(&mut out, author);
                out.push('}');
            }
            out.push_str("}\n");
        }

        // Body blocks
        for block in &doc.blocks {
            match block {
                Block::Heading { level, inlines } => {
                    let fs = match level {
                        1 => 44,
                        2 => 36,
                        3 => 28,
                        _ => 24,
                    };
                    out.push_str(&format!("\\pard\\s{level}\\sb200\\sa100\\b\\fs{fs} "));
                    write_inlines(&mut out, inlines);
                    out.push_str("\\b0\\fs24\\par\n");
                }
                Block::Paragraph { inlines } => {
                    out.push_str("\\pard\\sa100\\fs24 ");
                    write_inlines(&mut out, inlines);
                    out.push_str("\\par\n");
                }
                Block::CodeBlock { text, .. } => {
                    out.push_str("\\pard\\sa100\\li200{\\f1\\fs20 ");
                    for (i, line) in text.lines().enumerate() {
                        if i > 0 {
                            out.push_str("\\line ");
                        }
                        escape_rtf_str(&mut out, line);
                    }
                    out.push_str("}\\par\n");
                }
                Block::List {
                    ordered,
                    start,
                    items,
                } => {
                    let mut curr_num = *start;
                    for item in items {
                        out.push_str("\\pard\\li400 ");
                        if *ordered {
                            out.push_str(&format!("{curr_num}.\\tab "));
                            curr_num += 1;
                        } else {
                            out.push_str("\\bullet\\tab ");
                        }
                        for b in item {
                            if let Block::Paragraph { inlines } = b {
                                write_inlines(&mut out, inlines);
                            }
                        }
                        out.push_str("\\par\n");
                    }
                }
                Block::Table {
                    header,
                    rows,
                    align: _,
                } => {
                    let col_count = header.len();
                    if col_count > 0 {
                        // Header row
                        write_table_row_inlines(&mut out, header, true);
                        // Data rows
                        for row in rows {
                            write_table_row_inlines(&mut out, row, false);
                        }
                    }
                }
                Block::Blockquote { blocks } => {
                    out.push_str("\\pard\\li720\\i\\fs24 ");
                    for b in blocks {
                        if let Block::Paragraph { inlines } = b {
                            write_inlines(&mut out, inlines);
                            out.push_str("\\par\n");
                        }
                    }
                    out.push_str("\\i0\\li0\\par\n");
                }
                Block::Image { alt, .. } => {
                    out.push_str("\\pard\\sa100\\fs24 [Image: ");
                    escape_rtf_str(&mut out, alt);
                    out.push_str("]\\par\n");
                }
                Block::Rule => {
                    out.push_str(
                        "\\pard\\sa100\\fs24 ----------------------------------------\\par\n",
                    );
                }
            }
        }

        out.push('}');
        Ok(out.into_bytes())
    }
}

fn write_table_row_inlines(out: &mut String, cells: &[Vec<Inline>], is_header: bool) {
    out.push_str("\\trowd\\trautofit1\n");
    let mut right = 2000;
    for _ in cells {
        out.push_str(&format!("\\cellx{right}"));
        right += 2000;
    }
    out.push_str("\\pard\\intbl\\fs24 ");
    for cell in cells {
        if is_header {
            out.push_str("\\b ");
        }
        write_inlines(out, cell);
        if is_header {
            out.push_str("\\b0");
        }
        out.push_str("\\cell ");
    }
    out.push_str("\\row\n");
}

fn write_inlines(out: &mut String, inlines: &[Inline]) {
    for inline in inlines {
        match inline {
            Inline::Text(t) => escape_rtf_str(out, t),
            Inline::Strong(inner) => {
                out.push_str("{\\b ");
                write_inlines(out, inner);
                out.push('}');
            }
            Inline::Emph(inner) => {
                out.push_str("{\\i ");
                write_inlines(out, inner);
                out.push('}');
            }
            Inline::Strikethrough(inner) => {
                out.push_str("{\\strike ");
                write_inlines(out, inner);
                out.push('}');
            }
            Inline::Code(c) => {
                out.push_str("{\\f1\\fs20 ");
                escape_rtf_str(out, c);
                out.push('}');
            }
            Inline::Link { inlines, .. } => {
                write_inlines(out, inlines);
            }
        }
    }
}

fn escape_rtf_str(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '\n' => out.push_str("\\line "),
            '\t' => out.push_str("\\tab "),
            c if c.is_ascii() && !c.is_ascii_control() => out.push(c),
            c => {
                let code = c as u32;
                if code <= 0xFFFF {
                    out.push_str(&format!("\\u{}?", code as i16));
                } else {
                    // Encode surrogate pairs
                    let code = code - 0x10000;
                    let lead = (0xD800 + (code >> 10)) as u16;
                    let trail = (0xDC00 + (code & 0x3FF)) as u16;
                    out.push_str(&format!("\\u{}?\\u{}?", lead as i16, trail as i16));
                }
            }
        }
    }
}

fn normalize_inlines(inlines: Vec<Inline>) -> Vec<Inline> {
    let mut out: Vec<Inline> = Vec::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => {
                if t.is_empty() {
                    continue;
                }
                if let Some(Inline::Text(prev)) = out.last_mut() {
                    prev.push_str(&t);
                } else {
                    out.push(Inline::Text(t));
                }
            }
            Inline::Strong(mut inner) => {
                let norm = normalize_inlines(std::mem::take(&mut inner));
                if !norm.is_empty() {
                    out.push(Inline::Strong(norm));
                }
            }
            Inline::Emph(mut inner) => {
                let norm = normalize_inlines(std::mem::take(&mut inner));
                if !norm.is_empty() {
                    out.push(Inline::Emph(norm));
                }
            }
            Inline::Strikethrough(mut inner) => {
                let norm = normalize_inlines(std::mem::take(&mut inner));
                if !norm.is_empty() {
                    out.push(Inline::Strikethrough(norm));
                }
            }
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
fn inlines_to_plain_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => out.push_str(t),
            Inline::Code(c) => out.push_str(c),
            Inline::Strong(inner) | Inline::Emph(inner) | Inline::Strikethrough(inner) => {
                out.push_str(&inlines_to_plain_text(inner));
            }
            Inline::Link { inlines, .. } => {
                out.push_str(&inlines_to_plain_text(inlines));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rtf_roundtrip_preserves_document() {
        let doc = Document {
            meta: Metadata {
                title: Some("RTF Title".to_string()),
                author: Some("Jane Author".to_string()),
            },
            blocks: vec![
                Block::Heading {
                    level: 1,
                    inlines: vec![Inline::Text("Heading One".to_string())],
                },
                Block::Paragraph {
                    inlines: vec![
                        Inline::Text("Hello ".to_string()),
                        Inline::Strong(vec![Inline::Text("bold".to_string())]),
                        Inline::Text(" and ".to_string()),
                        Inline::Emph(vec![Inline::Text("italic".to_string())]),
                    ],
                },
                Block::Table {
                    header: vec![
                        vec![Inline::Text("Col1".to_string())],
                        vec![Inline::Text("Col2".to_string())],
                    ],
                    rows: vec![vec![
                        vec![Inline::Text("A".to_string())],
                        vec![Inline::Text("B".to_string())],
                    ]],
                    align: vec![Align::None, Align::None],
                },
            ],
        };

        let bytes = RtfWriter.write(&doc, Format::Rtf).expect("write RTF");
        assert!(bytes.starts_with(b"{\\rtf1"));

        let read_doc = RtfReader.read(&bytes, Format::Rtf).expect("read RTF");
        assert_eq!(read_doc.meta.title.as_deref(), Some("RTF Title"));
        assert_eq!(read_doc.meta.author.as_deref(), Some("Jane Author"));
        assert_eq!(read_doc.blocks.len(), 3);

        match &read_doc.blocks[0] {
            Block::Heading { level, inlines } => {
                assert_eq!(*level, 1);
                assert_eq!(inlines_to_plain_text(inlines), "Heading One");
            }
            other => panic!("expected Heading, got {other:?}"),
        }

        match &read_doc.blocks[1] {
            Block::Paragraph { inlines } => {
                assert_eq!(inlines_to_plain_text(inlines), "Hello bold and italic");
            }
            other => panic!("expected Paragraph, got {other:?}"),
        }

        match &read_doc.blocks[2] {
            Block::Table { header, rows, .. } => {
                assert_eq!(header.len(), 2);
                assert_eq!(inlines_to_plain_text(&header[0]), "Col1");
                assert_eq!(inlines_to_plain_text(&header[1]), "Col2");
                assert_eq!(rows.len(), 1);
                assert_eq!(inlines_to_plain_text(&rows[0][0]), "A");
                assert_eq!(inlines_to_plain_text(&rows[0][1]), "B");
            }
            other => panic!("expected Table, got {other:?}"),
        }
    }
}
