//! Diff command: semantic structural diff on the IDM.
//! Compares normalized document ASTs instead of rendered byte streams.

use std::fs;

use smelt_core::idm::{Block, Inline};
use smelt_core::{Format, read_document};

use crate::SmeltError;
use crate::cli::DiffArgs;

pub fn run(args: &DiffArgs) -> Result<(), SmeltError> {
    let b1 = fs::read(&args.file1)
        .map_err(|e| SmeltError::new(1, format!("cannot read {}: {}", args.file1.display(), e)))?;
    let b2 = fs::read(&args.file2)
        .map_err(|e| SmeltError::new(1, format!("cannot read {}: {}", args.file2.display(), e)))?;

    let fmt1 = smelt_detect::detect_path(&args.file1)
        .map(|d| d.format)
        .unwrap_or(Format::Text);
    let fmt2 = smelt_detect::detect_path(&args.file2)
        .map(|d| d.format)
        .unwrap_or(Format::Text);

    let doc1 = read_document(fmt1, &b1).map_err(|e| SmeltError::new(1, format!("diff: {}", e)))?;
    let doc2 = read_document(fmt2, &b2).map_err(|e| SmeltError::new(1, format!("diff: {}", e)))?;

    if doc1 == doc2 {
        println!("identical");
        return Ok(());
    }

    println!(
        "different ({} has {} blocks, {} has {} blocks):",
        args.file1.display(),
        doc1.blocks.len(),
        args.file2.display(),
        doc2.blocks.len()
    );

    // Metadata diff
    if doc1.meta != doc2.meta {
        if doc1.meta.title != doc2.meta.title {
            println!(
                "  [~] metadata.title: {:?} -> {:?}",
                doc1.meta.title, doc2.meta.title
            );
        }
        if doc1.meta.author != doc2.meta.author {
            println!(
                "  [~] metadata.author: {:?} -> {:?}",
                doc1.meta.author, doc2.meta.author
            );
        }
    }

    // Block sequence diff
    // shortcut: index-paired comparison, upgrade to Myers diff algorithm on IDM blocks if reordered blocks require tracking
    let max_len = doc1.blocks.len().max(doc2.blocks.len());
    for i in 0..max_len {
        match (doc1.blocks.get(i), doc2.blocks.get(i)) {
            (Some(b1), Some(b2)) if b1 != b2 => {
                println!("  [-] block {}: {}", i + 1, summarize_block(b1));
                println!("  [+] block {}: {}", i + 1, summarize_block(b2));
            }
            (Some(b1), None) => {
                println!("  [-] block {}: {}", i + 1, summarize_block(b1));
            }
            (None, Some(b2)) => {
                println!("  [+] block {}: {}", i + 1, summarize_block(b2));
            }
            _ => {}
        }
    }

    Ok(())
}

fn inlines_to_text(inlines: &[Inline]) -> String {
    let mut s = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) | Inline::Code(t) => s.push_str(t),
            Inline::Strong(children)
            | Inline::Emph(children)
            | Inline::Strikethrough(children)
            | Inline::Link {
                inlines: children, ..
            } => {
                s.push_str(&inlines_to_text(children));
            }
        }
    }
    s
}

fn summarize_block(b: &Block) -> String {
    match b {
        Block::Heading { level, inlines } => {
            let txt = inlines_to_text(inlines);
            format!("Heading(level {level}, \"{}\")", truncate_preview(&txt, 40))
        }
        Block::Paragraph { inlines } => {
            let txt = inlines_to_text(inlines);
            format!("Paragraph(\"{}\")", truncate_preview(&txt, 40))
        }
        Block::List {
            ordered,
            start,
            items,
        } => {
            format!(
                "List(ordered={ordered}, start={start}, items={})",
                items.len()
            )
        }
        Block::Table { header, rows, .. } => {
            format!("Table(cols={}, rows={})", header.len(), rows.len())
        }
        Block::CodeBlock { lang, text } => {
            format!(
                "CodeBlock(lang={}, lines={})",
                lang.as_deref().unwrap_or("none"),
                text.lines().count()
            )
        }
        Block::Image { src, alt } => {
            format!(
                "Image(src=\"{}\", alt=\"{}\")",
                truncate_preview(src, 30),
                truncate_preview(alt, 30)
            )
        }
        Block::Rule => "Rule".to_string(),
        Block::Blockquote { blocks } => {
            format!("Blockquote({} inner blocks)", blocks.len())
        }
    }
}

fn truncate_preview(s: &str, max_len: usize) -> String {
    let clean: String = s
        .chars()
        .map(|c| if c.is_whitespace() { ' ' } else { c })
        .collect();
    if clean.chars().count() <= max_len {
        clean
    } else {
        let truncated: String = clean.chars().take(max_len).collect();
        format!("{truncated}...")
    }
}
