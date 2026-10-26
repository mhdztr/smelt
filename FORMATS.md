# FORMATS.md -- smelt format matrix

Read/Write capability, engine, and target milestone for every planned
format. "Native" = pure Rust inside smelt. "Optional" = external tool via a
warm pool (see ARCHITECTURE.md); the tool is detected at runtime and never
required.

| Format | Read | Write | Engine | Milestone | Notes |
| --- | --- | --- | --- | --- | --- |
| Markdown (GFM/CommonMark) | yes | yes | native | M0 | comrak; tables, strikethrough, autolinks on; tasklists degrade (see lossy list) |
| HTML | yes | yes | native | M0 | scraper/html5ever read; hand-rolled serializer write; malformed input auto-repaired (lossy) |
| Plain text | yes | yes | native | M0 | blank-line paragraph splitting on read; minimal structure on write |
| CSV / TSV | yes | yes | native | M1 | first record is the header; each file becomes one strict IDM table; RFC-4180-style quoting |
| EPUB | yes | yes | native | M1 | zip + XHTML spine; reuses the HTML converter |
| DOCX | yes | yes | LibreOffice (optional) | M1 | via warm soffice pool |
| ODT | yes | yes | LibreOffice (optional) | M1 | via warm soffice pool |
| ODS | yes | yes | LibreOffice (optional) | M1 | OpenDocument spreadsheet; detected in M0, converted via soffice pool |
| ODP | yes | yes | LibreOffice (optional) | M1 | OpenDocument presentation; detected in M0, converted via soffice pool |
| PPTX | yes | yes | LibreOffice (optional) | M1 | slide-per-block; layout is lossy |
| XLSX | yes | yes | LibreOffice (optional) | M1 | sheets become tables; multi-sheet is lossy |
| PDF | yes | yes | PDFium (static FFI) | M2 | page-parallel via rayon |
| Searchable PDF | no | yes | PDFium (static) | M2 | renders text layer for OCR'd documents |
| LaTeX | yes | yes | pandoc (optional) | M3 | |
| man / troff | yes | yes | groff (optional) | M3 | |
| MOBI / AZW | yes | yes | calibre (optional) | M3 | |
| Images / scanned PDF (OCR) | yes | no | Tesseract 5 + Leptonica (static) | M3 | `smelt ocr` |
| RTF | planned | planned | native (M4) | M3+ | detection already ships in M0 |
| PDF (plain, via engine) | -- | -- | -- | -- | PDFium only; no external PDF engine |

Support levels: `yes` = implemented at the listed milestone; `planned` =
design agreed, code later; detection for all listed formats ships in M0 via
`smelt-detect` even before converters exist (so `smelt info` can identify
them and `smelt convert` can fail with the precise "no native reader" exit-3
message).

## Detection rules (M0, implemented)

Magic bytes first; extension is only a tiebreak/disambiguator.

1. `%PDF-` at offset 0 -> PDF (certain).
2. `PK\x03\x04` at offset 0 -> ZIP family; scan the raw prefix for stored
   entry names:
   - `mimetypeapplication/epub+zip` -> EPUB
   - `word/document.xml` -> DOCX
   - `ppt/presentation.xml` -> PPTX
   - `xl/workbook.xml` -> XLSX
   - `mimetype` + `content.xml`, no OOXML marker -> ODT; the stored ODF
     mimetype refines this to ODS (spreadsheet) or ODP (presentation) when
     present, so exit-3 messages name the right subformat
   - nothing matched -> unknown ("zip archive (unrecognized layout)")
3. `{\rtf` at offset 0 (<= 32 bytes of leading whitespace/BOM skipped) -> RTF.
4. Case-insensitive scan of the first 1 KiB for `<!doctype html`, `<html`,
   `<head`, `<body` (after optional BOM/whitespace/XML declaration) -> HTML
   (certain with doctype). A leading `<!--` comment or `<!doctype` is also
   certain. Otherwise a leading HTML tag (`<p>`, `<div>`, `<table>`,
   `<span>`, ... or a stray closer) -> HTML guessed. The tag test requires
   `<` + optional `/` + an ASCII letter and a closing `>` in the scanned
   head, so prose (`a < b`, an unclosed `<name`, C++ `vector<int>`) stays
   text. **Extension guard:** a `.txt`/`.text` file only takes the HTML
   branch when a structural marker (`<!doctype`, `<html`, `<head`, `<body`)
   is present; loose leading tags there stay plain text, so a log line such
   as `<error> disk full` is not mistaken for a document.
5. Valid UTF-8 in the read prefix -> text family, by extension:
   `.md`/`.markdown`/`.mdown` -> Markdown; `.csv` -> CSV; `.tsv` -> TSV;
   `.html`/`.htm`/`.xhtml` -> HTML (a file with no markup in its prefix is
   still HTML by extension); `.txt`/`.text`/none/anything-else -> plain
   text. All guessed -- markdown has no magic bytes. Valid UTF-8 is never
   reported as unknown.
6. Invalid UTF-8 and no magic match -> unknown.

## Known lossy cases (M0)

- **Tasklist checkboxes**: `- [ ]` / `- [x]` become plain list items (IDM v0
  has no checkbox node; reserved for a future `Task` marker).
- **Setext headings** (`===` / `---` underline): parsed correctly by comrak,
  but the smelt writer always emits ATX (`#`) headings.
- **Malformed HTML**: html5ever repairs it silently; the repaired tree is
  what reaches the IDM.
- **HTML-only styling** (`<span style>`, classes, inline styles): dropped;
  the IDM carries structure, not presentation.
- **Loose vs tight lists**: both parse, the writer emits tight style.
- **RTL text**: content is preserved exactly (including combining marks) but
  M0 performs no shaping or bidi reordering -- rendering is the consumer's
  job.
- **Leading tag-like lines in `.txt`**: a `.txt` file is sometimes read
  through the Markdown reader (the plain-text source rule in `docs/cli.md`),
  and Markdown parses a line that opens with an HTML tag as a raw HTML block.
  A log line such as `<error> disk full` therefore loses the `<error>` marker
  when converted. Detection no longer misclassifies the file as HTML, but the
  content of a tag-like leading line is still not preserved verbatim; use a
  non-Markdown `.txt` consumer or remove the angle brackets when the marker
  must survive.
- **Plain text input**: a `.txt` (or any bytes detection lands on plain
  `Text`) is read through the **Markdown reader** (the plain-text source rule
  in `docs/cli.md`), so Markdown constructs -- ATX headings, lists,
  emphasis, code -- are interpreted rather than preserved verbatim. Text
  with no Markdown syntax converts as paragraphs.
- **Soft and hard breaks** (`LineBreak`): both collapse to a single space.
  IDM v0 has no break node, and a space roundtrips identically through HTML
  and plain text (`formats/markdown.rs`).
- **HTML whitespace collapse**: whitespace runs collapse to one space and
  block edges trim, per HTML rendering semantics. Non-breaking space
  (U+00A0) is content, not whitespace, and survives every converter
  (`formats/mod.rs`).
- **Front matter is a key subset**: only `title` and `author` reach
  `Metadata`; every other key and all comments are dropped. Quoted scalars
  are unescaped minimally (backslash escapes only) -- front matter is not
  parsed as full YAML.
- **Plain-text newline collapse**: a paragraph block's internal newlines
  become spaces (blank-line splitting only); line structure inside a
  paragraph is not preserved.
- **Raw HTML blocks in Markdown**: parsed through the same fragment walker
  as standalone HTML documents (`formats::html::fragment_blocks`), so raw
  HTML in Markdown inherits HTML's repair and whitespace rules.
- **CSV / TSV shape**: the first record is the IDM table header and every later
  record must have the same number of fields. Metadata and non-table blocks
  have no delimited representation; the writer rejects documents that are not
  exactly one table. Inline styling markers are rendered as cell text and
  link destinations are appended as `text (destination)`.
