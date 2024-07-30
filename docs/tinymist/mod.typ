
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
