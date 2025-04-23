= Example Document Title (Level 1 Heading)

This is an example file containing all elements, used to test the conversion from Typst to other formats.

== Formatted Text (Level 2 Heading)

This is *bold text* and _italic text_, along with some #highlight[highlighted text] and #strike[strikethrough text].

Line break test: \
This line should appear immediately below the previous one.

=== Paragraphs and Quotes (Level 3 Heading)

Below is an example of paragraph separation:

This is the first paragraph.

This is the second paragraph.

#quote(attribution: "Some Author")[This is a quote block.]

==== Lists (Level 4 Heading)

Ordered list:
1. First item
2. Second item
  1. First nested subitem
  2. Second nested subitem
3. Third item

Unordered list:
- Item one
- Item two
  - Nested item A
  - Nested item B
- Item three

Mixed ordered and unordered lists:
1. Ordered item one
  - Unordered subitem
  - Another unordered subitem
2. Ordered item two
  1. Ordered subitem
  2. Another ordered subitem

===== Code (Level 5 Heading)

Inline code: `print("Hello World")`

```rust
fn main() {
    println!("This is a Rust code block");
}
```

====== Links and References (Level 6 Heading)

This is a [link text](https://example.com).

#figure(
  image("image.png", alt: "Example image"),
  caption: "Example of an image with a caption",
)<ref-example>
Referencing previous content: #ref(<ref-example>)

== Images and Tables

#image("image.png", alt: "Standalone image")

=== Tables

#table(
  columns: 3,
  [Header 1], [Header 2], [Header 3],
  [Row 1 Cell 1], [Row 1 Cell 2], [Row 1 Cell 3],
  [Row 2 Cell 1], [Row 2 Cell 2], [Row 2 Cell 3],
)

=== Grid

#grid(
  columns: 2,
  [Grid Cell 1], [Grid Cell 2],
  [Grid Cell 3], [Grid Cell 4],
)

== Mathematical Formulas

Inline formula: $a^2 + b^2 = c^2$

Block-level formula:
$ sum_(i=1)^n i = frac(n(n+1), 2) $

== Outline

#outline()
