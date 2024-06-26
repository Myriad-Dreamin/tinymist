// The project function defines how your document looks.
// It takes your content and some metadata and formats it.
// Go ahead and customize it to your liking!
#let project(title: "", authors: (), mode: "paged", body) = {
  assert(mode in ("paged", "pageless"), message: "invalid mode")

  // Set the document's basic properties.
  set document(author: authors, title: title)
  set page(
    height: auto,
    width: 210mm,
    // numbering: "1", number-align: center,
  ) if mode == "pageless"
  set text(font: ("Linux Libertine", "Source Han Serif SC", "Source Han Sans"), size: 12pt, lang: "en")
  set page(height: 297mm)


  // Title row.
  align(center)[
    #block(text(weight: 700, 1.75em, title))
  ]

  // Main body.
  set par(justify: true)

  // rules
  show raw.where(block: true): set par(justify: false)
  show raw.where(block: true): rect.with(width: 100%, radius: 2pt, fill: luma(240), stroke: 0pt)

  show link: text.with(blue)
  show link: underline

  body
}
