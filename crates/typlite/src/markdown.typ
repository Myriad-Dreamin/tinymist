#let bool-str(x) = {
  if x {
    "true"
  } else {
    "false"
  }
}

// typst doesn't allow things like `typParbreak`.
#let md-parbreak = html.elem("m1parbreak", "")
#let md-linebreak = html.elem("m1linebreak", "")
#let md-strong(body, delta: 0) = html.elem("span", html.elem("m1strong", body))
#let md-emph(body) = html.elem("span", html.elem("m1emph", body))
#let md-highlight(body) = html.elem("span", html.elem("m1highlight", body))
#let md-strike(body) = html.elem("span", html.elem("m1strike", body))
#let md-raw(lang: none, block: false, text) = {
  let body = html.elem(
    "m1raw",
    attrs: (
      lang: if lang == none {
        ""
      } else {
        lang
      },
      block: bool-str(block),
      text: text,
    ),
    "",
  )

  if block {
    return body
  } else {
    html.elem("span", body)
  }
}
#let md-link(dest: none, body) = html.elem(
  "span",
  html.elem(
    "m1link",
    attrs: (dest: dest),
    body,
  ),
)
#let md-label(dest: none, body) = html.elem(
  "m1label",
  attrs: (dest: dest),
  body,
)
#let md-ref(body) = html.elem(
  "span",
  html.elem(
    "m1ref",
    body,
  ),
)
#let md-heading(level: int, body) = html.elem(
  "m1heading",
  attrs: (level: str(level)),
  box(body),
)
#let md-outline = html.elem.with("m1outline")
#let md-outline-entry(level: int, body) = html.elem(
  "m1outentry",
  attrs: (level: str(level)),
  body,
)
#let md-quote(attribution: none, body) = html.elem(
  "m1quote",
  attrs: (attribution: attribution),
  body,
)
#let md-table(it) = html.elem(
  "m1table",
  it,
)
#let md-grid(columns: auto, ..children) = html.elem(
  "m1grid",
  table(columns: columns, ..children.pos().map(it => table.cell(it.body))),
)
#let md-image(src: "", alt: none) = html.elem(
  "m1image",
  attrs: (
    src: src,
    alt: if alt == none {
      ""
    } else {
      alt
    },
  ),
  "",
)
#let md-figure(body, caption: none) = html.elem(
  "m1figure",
  attrs: (
    caption: if caption == none {
      ""
    } else {
      if caption.func() == text {
        caption.text
      } else {
        ""
      }
    },
  ),
  body,
)

#let md-doc(body) = {
  // show block: it => md-block(it)
  // distinguish parbreak from <p> tag
  show parbreak: md-parbreak
  show strong: it => md-strong(it.body, delta: it.delta)
  show emph: it => md-emph(it.body)
  show highlight: md-highlight
  show strike: md-strike
  // todo: icc?
  show image: it => md-image(src: it.source, alt: it.alt)

  show raw: it => md-raw(lang: it.lang, block: it.block, it.text)
  show link: it => md-link(dest: it.dest, it.body)
  // show label: it => html.elem("m1Label", it)
  show ref: it => md-ref(it)

  show heading: it => md-heading(level: it.level, it.body)
  show outline: md-outline
  show outline.entry: it => md-outline-entry(level: it.level, it.element)
  show quote: it => html.elem(
    "m1quote",
    attrs: (
      attribution: it.attribution,
    ),
    it.body,
  )
  show quote: it => md-quote(attribution: it.attribution, it.body)
  show table: it => md-table(it)
  show grid: it => md-grid(columns: it.columns, ..it.children)

  show math.equation.where(block: false): it => html.elem("m1eqinline", html.frame(box(inset: 0.5em, it)))
  show math.equation.where(block: true): it => html.elem("m1eqblock", html.frame(block(inset: 0.5em, it)))

  show linebreak: md-linebreak // math equation may include linebreak
  show figure: it => md-figure(it.body, caption: it.caption.body)

  html.elem("m1document", body)
}
