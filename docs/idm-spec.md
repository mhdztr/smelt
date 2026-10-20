# docs/idm-spec.md -- Intermediate Document Model specification

**IDM version: 0**

The IDM is smelt's normalized intermediate representation: a small,
lossy-by-design AST that every reader produces and every writer consumes.
This document is the spec of record and mirrors
`crates/smelt-core/src/idm.rs`; any code change updates both in the same
commit.

## Serialization note

The serde representation (JSON-compatible derived forms) is the future
payload for the conversion cache and semantic diff. It is **not yet
stability-guaranteed**: field names and enum shapes may change until the
IDM version is bumped. Consumers outside smelt should not persist it.

## Types

### Document

| Field | Type | Meaning |
| --- | --- | --- |
| `meta` | `Metadata` | document metadata |
| `blocks` | `Vec<Block>` | top-level block sequence, in document order |

### Metadata

| Field | Type | Meaning |
| --- | --- | --- |
| `title` | `Option<String>` | document title (HTML `<title>`, md front matter `title`) |
| `author` | `Option<String>` | author (HTML `<meta name="author">`, md front matter `author`) |

Metadata channel per format (M0):

| Format | Read | Write |
| --- | --- | --- |
| Markdown | `---` front matter block: `title` and `author` keys only; other keys and comments are dropped, not an error | front matter block emitted when `title` or `author` is set |
| HTML | `<title>` and `<meta name="author">` when present | `<title>` and `<meta name="author">` when set |
| Plain text | none -- the format has no metadata channel, so `Metadata` is always default on read | none -- `Metadata` drops silently (lossy, see FORMATS.md) |

### Block

| Variant | Fields | Invariants |
| --- | --- | --- |
| `Heading` | `level: u8`, `inlines: Vec<Inline>` | `level` in `1..=6` |
| `Paragraph` | `inlines: Vec<Inline>` | |
| `List` | `ordered: bool`, `start: u64`, `items: Vec<Vec<Block>>` | nesting is expressed by an inner `List` block inside an item -- no depth field exists; `start` is the first ordinal (1 for ordered defaults, 0 accepted for unordered) |
| `Table` | `header: Vec<Vec<Inline>>`, `rows: Vec<Vec<Vec<Inline>>>`, `align: Vec<Align>` | `align.len() == header.len()`; every row has `header.len()` cells; a cell is a sequence of inlines |
| `CodeBlock` | `lang: Option<String>`, `text: String` | `text` has no trailing fence; `lang` is the info string's first word |
| `Image` | `src: String`, `alt: String` | images are leaf blocks in v0 (inline images inside paragraphs are promoted to a following `Image` block) |
| `Rule` | | thematic break (`<hr>`, `---`) |
| `Blockquote` | `blocks: Vec<Block>` | arbitrary nested blocks |

### Inline

| Variant | Fields | Notes |
| --- | --- | --- |
| `Text` | `String` | literal text; whitespace preserved as parsed |
| `Code` | `String` | inline code span |
| `Strong` | `Vec<Inline>` | nesting allowed |
| `Emph` | `Vec<Inline>` | nesting allowed |
| `Strikethrough` | `Vec<Inline>` | GFM `~~x~~` / `<del>` |
| `Link` | `dest: String`, `title: Option<String>`, `inlines: Vec<Inline>` | autolinks become `Link` with the URL as text |

There is deliberately no `Raw`/`Html` inline: raw HTML in Markdown input is
either parsed into structure or dropped (documented lossy case in
FORMATS.md).

### Align

`None`, `Left`, `Center`, `Right` -- table column alignment (GFM `:---`
syntax, HTML `align`/`style` attribute).

## Invariants (checked by tests)

1. Heading levels are `1..=6`.
2. Table column counts agree across header, alignment, and every row.
3. List nesting uses inner `List` blocks; there is no depth counter.
4. `Document` values implement `Debug + Clone + PartialEq + Serialize +
   Deserialize`; roundtrip tests compare `Document` values, never raw text.

## Non-goals for v0

| Concept | Status | Reserved future names |
| --- | --- | --- |
| Footnotes | not in v0 | `Block::Footnote`, `Inline::FootnoteRef` |
| Math (inline/display) | not in v0 | `Inline::Math`, `Block::Math` |
| Raw blocks (latex/html passthrough) | not in v0 | `Block::Raw` |
| Task-list checkboxes | not in v0 | `Block::Task` or a `checked` flag on list items |
| Definition lists | not in v0 | `Block::DefinitionList` |
| Inline images | promoted to blocks in v0 | `Inline::Image` |
| Character-level styling beyond strong/emph/code/strike | not in v0 | `Inline::Underline`, `Inline::SupSub` |

These names are reserved: adding a variant under a different name requires
a version bump and a note here.
