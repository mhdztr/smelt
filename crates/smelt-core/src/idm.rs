//! The Intermediate Document Model (IDM), version 0.
//!
//! Spec of record: `docs/idm-spec.md`. Any change to the types below updates
//! that document in the same commit.

use serde::{Deserialize, Serialize};

/// A parsed document: metadata plus a sequence of top-level blocks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub meta: Metadata,
    pub blocks: Vec<Block>,
}

/// Document-level metadata (v0: title and author only).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Metadata {
    pub title: Option<String>,
    pub author: Option<String>,
}

/// A block-level element. Nesting is expressed by inner `Vec<Block>` fields
/// (lists, blockquotes); there is no depth counter anywhere.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Block {
    Heading {
        /// `1..=6`.
        level: u8,
        inlines: Vec<Inline>,
    },
    Paragraph {
        inlines: Vec<Inline>,
    },
    List {
        ordered: bool,
        /// First ordinal for ordered lists (1 normally); 0 accepted for
        /// unordered lists.
        start: u64,
        /// Each item is itself a block sequence; nesting uses an inner
        /// `Block::List` inside an item.
        items: Vec<Vec<Block>>,
    },
    Table {
        header: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
        /// One entry per column; `align.len() == header.len()`.
        align: Vec<Align>,
    },
    CodeBlock {
        /// First word of the info string.
        lang: Option<String>,
        text: String,
    },
    /// Leaf image reference (inline images are promoted to a following
    /// `Image` block by readers in v0).
    Image {
        src: String,
        alt: String,
    },
    /// Thematic break (`---`, `<hr>`).
    Rule,
    Blockquote {
        blocks: Vec<Block>,
    },
}

/// An inline-level element. `Strong`/`Emph`/`Strikethrough`/`Link` nest.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Inline {
    Text(String),
    Code(String),
    Strong(Vec<Inline>),
    Emph(Vec<Inline>),
    Strikethrough(Vec<Inline>),
    Link {
        dest: String,
        title: Option<String>,
        inlines: Vec<Inline>,
    },
}

/// Table column alignment (GFM `:---` syntax, HTML `align` attribute).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Align {
    None,
    Left,
    Center,
    Right,
}

impl Document {
    /// Check the structural invariants every reader must uphold, returning an
    /// actionable message for the first violation found.
    /// These are the guarantees the rest of the pipeline relies on (writers
    /// index table columns by `align`, renderers index headings by `level`),
    /// so a reader that violates one would otherwise panic far from the
    /// cause. Checked over the whole block tree, including blocks nested in
    /// lists and blockquotes.
    pub fn validate(&self) -> Result<(), String> {
        fn check_blocks(blocks: &[Block]) -> Result<(), String> {
            for block in blocks {
                match block {
                    Block::Heading { level, .. } => {
                        if !(1..=6).contains(level) {
                            return Err(format!("heading level out of range: {level}"));
                        }
                    }
                    Block::Table {
                        header,
                        rows,
                        align,
                    } => {
                        if align.len() != header.len() {
                            return Err(format!(
                                "table alignment count {} != column count {}",
                                align.len(),
                                header.len()
                            ));
                        }
                        for (i, row) in rows.iter().enumerate() {
                            if row.len() != header.len() {
                                return Err(format!(
                                    "table row {i} has {} cells, expected {}",
                                    row.len(),
                                    header.len()
                                ));
                            }
                        }
                    }
                    Block::List { items, .. } => {
                        for item in items {
                            check_blocks(item)?;
                        }
                    }
                    Block::Blockquote { blocks } => check_blocks(blocks)?,
                    Block::Paragraph { .. }
                    | Block::CodeBlock { .. }
                    | Block::Image { .. }
                    | Block::Rule => {}
                }
            }
            Ok(())
        }
        check_blocks(&self.blocks)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_accepts_the_reference_document() {
        assert_eq!(sample().validate(), Ok(()));
    }

    #[test]
    fn validate_rejects_out_of_range_heading_levels() {
        let mut doc = sample();
        doc.blocks = vec![Block::Heading {
            level: 7,
            inlines: vec![Inline::Text("too deep".into())],
        }];
        let err = doc.validate().expect_err("level 7 is invalid");
        assert!(err.contains("heading level"), "{err}");
    }

    #[test]
    fn validate_rejects_ragged_tables() {
        let mut doc = sample();
        doc.blocks = vec![Block::Table {
            header: vec![
                vec![Inline::Text("a".into())],
                vec![Inline::Text("b".into())],
            ],
            rows: vec![vec![vec![Inline::Text("only one".into())]]],
            align: vec![Align::None, Align::None],
        }];
        let err = doc.validate().expect_err("ragged row is invalid");
        assert!(err.contains("row 0"), "{err}");
    }

    #[test]
    fn validate_rejects_alignment_column_mismatch() {
        let mut doc = sample();
        doc.blocks = vec![Block::Table {
            header: vec![vec![Inline::Text("a".into())]],
            rows: vec![],
            align: vec![Align::None, Align::None],
        }];
        let err = doc.validate().expect_err("alignment mismatch is invalid");
        assert!(err.contains("alignment"), "{err}");
    }

    #[test]
    fn validate_walks_nested_blocks() {
        // A bad heading hidden inside a list item must still be caught: the
        // invariant applies to the whole tree, not just top-level blocks.
        let mut doc = sample();
        doc.blocks = vec![Block::List {
            ordered: false,
            start: 0,
            items: vec![vec![Block::Blockquote {
                blocks: vec![Block::Heading {
                    level: 0,
                    inlines: vec![],
                }],
            }]],
        }];
        let err = doc.validate().expect_err("nested level 0 is invalid");
        assert!(err.contains("heading level"), "{err}");
    }

    fn sample() -> Document {
        Document {
            meta: Metadata {
                title: Some("T\u{00ed}tle".into()),
                author: Some("A. Author".into()),
            },
            blocks: vec![
                Block::Heading {
                    level: 2,
                    inlines: vec![Inline::Text("head".into())],
                },
                Block::Paragraph {
                    inlines: vec![
                        Inline::Strong(vec![Inline::Text("bold".into())]),
                        Inline::Text(" and ".into()),
                        Inline::Emph(vec![Inline::Text("em".into())]),
                        Inline::Strikethrough(vec![Inline::Text("gone".into())]),
                        Inline::Code("k=v".into()),
                        Inline::Link {
                            dest: "https://example.com".into(),
                            title: None,
                            inlines: vec![Inline::Text("site".into())],
                        },
                    ],
                },
                Block::List {
                    ordered: true,
                    start: 3,
                    items: vec![vec![
                        Block::Paragraph {
                            inlines: vec![Inline::Text("item".into())],
                        },
                        Block::List {
                            ordered: false,
                            start: 0,
                            items: vec![vec![Block::Paragraph {
                                inlines: vec![Inline::Text("nested".into())],
                            }]],
                        },
                    ]],
                },
                Block::Table {
                    header: vec![vec![Inline::Text("a".into())]],
                    rows: vec![vec![vec![Inline::Text(
                        "\u{0627}\u{0644}\u{0639}\u{0631}\u{0628}\u{064a}\u{0629}".into(),
                    )]]],
                    align: vec![Align::Center],
                },
                Block::CodeBlock {
                    lang: Some("rust".into()),
                    text: "fn main() {}\n".into(),
                },
                Block::Image {
                    src: "img.png".into(),
                    alt: "an image".into(),
                },
                Block::Rule,
                Block::Blockquote {
                    blocks: vec![Block::Paragraph {
                        inlines: vec![Inline::Text("quoted".into())],
                    }],
                },
            ],
        }
    }

    #[test]
    fn serde_json_roundtrip_preserves_document() {
        let doc = sample();
        let json = serde_json::to_string(&doc).expect("serialize");
        let back: Document = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(doc, back);
    }

    /// Every reader must pass [`Document::validate`]; the fixture is the
    /// shared reference document, so this exercises the same production
    /// invariant check the pipeline runs (see `reader_doc` in `lib.rs`).
    #[test]
    fn validate_covers_the_whole_fixture() {
        let doc = sample();
        doc.validate()
            .expect("the shared fixture must satisfy the IDM invariants");
        assert!(doc.blocks.len() > 5, "fixture should be non-trivial");
    }
}
