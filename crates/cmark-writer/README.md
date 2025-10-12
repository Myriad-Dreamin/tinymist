# cmark-writer

[![License: MIT](https://img.shields.io/badge/License-MIT-green.svg)](./LICENSE)

A CommonMark writer implementation in Rust.

## Basic Usage

```rust
use cmark_writer::ast::{Node, ListItem};
use cmark_writer::writer::CommonMarkWriter;

// Create a document
let document = Node::Document(vec![
    Node::heading(1, vec![Node::Text("Hello CommonMark".into())]),
    Node::Paragraph(vec![
        Node::Text("This is a simple ".into()),
        Node::Strong(vec![Node::Text("example".into())]),
        Node::Text(".".into()),
    ]),
]);

// Render to CommonMark
let mut writer = CommonMarkWriter::new();
writer.write(&document).expect("Failed to write document");
let markdown = writer.into_string();

println!("{}", markdown);
```

## Custom Options

```rust
use cmark_writer::options::WriterOptionsBuilder;
use cmark_writer::writer::CommonMarkWriter;

// Use builder pattern for custom options
let options = WriterOptionsBuilder::new()
    .strict(true)
    .hard_break_spaces(false)
    .indent_spaces(2)
    .build();

let mut writer = CommonMarkWriter::with_options(options);
```

## Table Support

```rust
use cmark_writer::ast::{Node, tables::TableBuilder};

// Create tables with the builder pattern
let table = TableBuilder::new()
    .headers(vec![
        Node::Text("Name".into()), 
        Node::Text("Age".into())
    ])
    .add_row(vec![
        Node::Text("John".into()),
        Node::Text("30".into()),
    ])
    .add_row(vec![
        Node::Text("Alice".into()),
        Node::Text("25".into()),
    ])
    .build();
```

## GitHub Flavored Markdown (GFM)

Enable GFM features by adding to your `Cargo.toml`:

```toml
[dependencies]
cmark-writer = { version = "0.8.0", features = ["gfm"] }
```

GFM Support:

- Tables with column alignment
- Strikethrough text
- Task lists
- Extended autolinks
- HTML element filtering

## HTML Writing

The library provides dedicated HTML writing capabilities through the `HtmlWriter` class:

```rust
use cmark_writer::{HtmlWriter, HtmlWriterOptions, Node};

// Create HTML writer with custom options
let options = HtmlWriterOptions {
    strict: true,
    code_block_language_class_prefix: Some("language-".into()),
    #[cfg(feature = "gfm")]
    enable_gfm: true,
    #[cfg(feature = "gfm")]
    gfm_disallowed_html_tags: vec!["script".into()],
};

let mut writer = HtmlWriter::with_options(options);

// Write some nodes
let paragraph = Node::Paragraph(vec![Node::Text("Hello HTML".into())]);
writer.write_node(&paragraph).unwrap();

// Get resulting HTML
let html = writer.into_string().unwrap();
assert_eq!(html, "<p>Hello HTML</p>\n");
```

## Custom Nodes

```rust
use cmark_writer::{HtmlWriteResult, HtmlWriter, Node, WriteResult};
use cmark_writer::writer::InlineWriterProxy;
use cmark_writer::custom_node;
use ecow::EcoString;

#[derive(Debug, Clone, PartialEq)]
#[custom_node(block=false, html_impl=true)]
struct HighlightNode {
    content: EcoString,
    color: EcoString,
}

impl HighlightNode {
    // Implementation for CommonMark output
    fn write_custom(&self, writer: &mut InlineWriterProxy) -> WriteResult<()> {
        writer.write_str("<span style=\"background-color: ")?;
        writer.write_str(&self.color)?;
        writer.write_str("\">")?;
        writer.write_str(&self.content)?;
        writer.write_str("</span>")?;
        Ok(())
    }
    
    // Optional HTML-specific implementation
    fn write_html_custom(&self, writer: &mut HtmlWriter) -> HtmlWriteResult<()> {
        writer.start_tag("span")?;
        writer.attribute("style", &format!("background-color: {}", self.color))?;
        writer.finish_tag()?;
        writer.text(&self.content)?;
        writer.end_tag("span")?;
        Ok(())
    }
}
```

## Development

```bash
# Build
cargo build

# Run tests
cargo test
```

## License

This project is licensed under the MIT License - see the LICENSE file for details.
