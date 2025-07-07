
#import "/docs/tinymist/book.typ": book-page, cross-link
#import "/typ/templates/page.typ": *
#import "/typ/templates/git.typ": *
#import "@preview/fletcher:0.5.6" as fletcher: *
#import "@preview/numbly:0.1.0": numbly

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

#let kbd = if is-md-target {
  html.elem.with("kbd")
} else {
  raw
}
#import fletcher.shapes: diamond

#let fg-blue = main-color.mix(rgb("#0074d9"))
#let pro-tip(content) = context if sys.inputs.at("x-target", default: none) == "md" {
  html.elem("m1alerts", attrs: ("class": "note"), content)
} else {
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
}

// todo: use theme-box, to solve theme issue of typst figures.
#let cond-image(img) = context if shiroa-sys-target() == "html" {
  theme-box(class: "pseudo-image", theme => {
    show raw.where(tab-size: 114): with-raw-theme.with(theme.style.code-theme)
    set text(fill: theme.main-color)
    set line(stroke: theme.main-color)
    html.frame(img(theme))
  })
} else {
  theme-box(img)
}

#let fletcher-ctx(theme, node-shape: fletcher.shapes.hexagon) = {
  (
    if theme.is-dark {
      (rgb("#66ccffa0"), rgb("#b0a4e3a0"), rgb("#a4e2c690"))
    } else {
      (rgb("#66ccffcf"), rgb("#b0a4e3cf"), rgb("#a4e2c690"))
    },
    node.with(shape: node-shape, stroke: theme.main-color),
    edge.with(stroke: theme.main-color),
  )
}

#let note-box = pro-tip
