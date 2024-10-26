
#import "/docs/tinymist/book.typ": book-page, cross-link
#import "/typ/templates/page.typ": *
#import "@preview/fletcher:0.4.4" as fletcher: *

/// This function is to render a text string in monospace style and function
/// color in your defining themes.
///
/// ## Examples
///
/// ```typc
/// typst-func("list.item")
/// ```
///
/// Note: it doesn't check whether input is a valid function identifier or path.
#let typst-func(it) = [
  #raw(it + "()", lang: "typc") <typst-raw-func>
]

#let kbd = raw
#let md-alter(left, right) = left

#let colors = (blue.lighten(10%), olive, eastern)
#import fletcher.shapes: diamond

#let fg-blue = main-color.mix(rgb("#0074d9"))
#let pro-tip(content) = (
  context {
    block(
      width: 100%,
      breakable: false,
      inset: (x: 0.65em, y: 0.65em, left: 0.65em * 0.6),
      radius: 4pt,
      fill: rgb("#0074d920"),
      {
        set text(fill: fg-blue)
        content
      },
    )
  }
)

#let note-box = pro-tip
