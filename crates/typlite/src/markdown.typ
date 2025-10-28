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
#let md-raw(lang: none, block: false, text: "", body) = {
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
    body,
  )

  if block {
    return body
  } else {
    html.elem("span", body)
  }
}

#let normalize-link-dest(dest) = {
  if dest == none {
    ""
  } else if type(dest) == str {
    dest
  } else if type(dest) == label {
    "#" + str(dest)
  } else {
    str(dest)
  }
}

#let md-link(dest: none, body) = html.elem(
  "span",
  html.elem(
    "m1link",
    attrs: (dest: normalize-link-dest(dest)),
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
#let md-quote(/* attribution: none, */ body) = html.elem(
  "m1quote",
  // attrs: (attribution: attribution),
  body,
)
#let md-table(it) = html.elem(
  "m1table",
  it,
)
#let md-grid(columns: auto, ..children) = html.elem(
  "m1grid",
  {
    let children = children.pos()
    let header = if children.first().func() == grid.header {
      (table.header(..children.first().children.map(cell => table.cell(cell.body))),)
      children = children.slice(1)
    } else {
      ()
    }
    let footer = if children.last().func() == grid.footer {
      (table.footer(..children.last().children.map(cell => table.cell(cell.body))),)
      children = children.slice(0, -1)
    } else {
      ()
    }

    table(columns: columns, ..header, ..children.map(it => table.cell(it)), ..footer)
  },
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
      if caption.body.func() == text {
        caption.body.text
      } else {
        ""
      }
    },
  ),
  body,
)

#let if-not-paged(it, act) = {
  if target() == "html" {
    act
  } else {
    it
  }
}

#let example(code) = {
  let lang = if code.has("lang") and code.lang != none { code.lang } else { "typ" }

  let lines = code.text.split("\n")
  let display = ""
  let compile = ""

  for line in lines {
    if line.starts-with(">>>") {
      compile += line.slice(3) + "\n"
    } else if line.starts-with("<<< ") {
      display += line.slice(4) + "\n"
    } else {
      display += (line + "\n")
      compile += (line + "\n")
    }
  }

  let result = raw(block: true, lang: lang, display)

  result
  if sys.inputs.at("x-remove-html", default: none) != "true" {
    let mode = if lang == "typc" { "code" } else { "markup" }

    html.elem(
      "m1idoc",
      attrs: (src: compile, mode: mode),
    )
  }
}

#let process-math-eq(item) = {
  if type(item) == str {
    return item
  }
  if type(item) == array {
    if (
      item.any(x => {
        type(x) == content and x.func() == str
      })
    ) {
      item.flatten()
    } else {
      item.map(x => process-math-eq(x)).flatten()
    }
  } else {
    process-math-eq(item.fields().values().flatten().filter(x => type(x) == content or type(x) == str))
  }
}

#let md-doc(body) = context {
  // distinguish parbreak from <p> tag
  show parbreak: it => if-not-paged(it, md-parbreak)
  show strong: it => if-not-paged(it, md-strong(it.body, delta: it.delta))
  show emph: it => if-not-paged(it, md-emph(it.body))
  show highlight: it => if-not-paged(it, md-highlight(it))
  show strike: it => if-not-paged(it, md-strike(it))
  // todo: icc?
  show image: it => if-not-paged(it, md-image(src: it.source, alt: it.alt))

  show raw: it => if-not-paged(it, md-raw(lang: it.lang, block: it.block, text: it.text, it))
  show link: it => if-not-paged(it, md-link(dest: it.dest, it.body))
  show ref: it => if-not-paged(it, md-ref(it))

  show heading: it => if-not-paged(it, md-heading(level: it.level, it.body))
  show outline: it => if-not-paged(it, md-outline(it))
  show outline.entry: it => if-not-paged(it, md-outline-entry(level: it.level, it.element))
  show quote: it => if-not-paged(it, md-quote(it.body))
  show table: it => if-not-paged(it, md-table(it))
  show grid: it => if-not-paged(it, md-grid(columns: it.columns, ..it.children))

  show math.equation.where(block: false): it => if-not-paged(
    it,
    html.elem(
      "m1eqinline",
      if sys.inputs.at("x-remove-html", default: none) != "true" { html.frame(box(inset: 0.5em, it)) } else {
        process-math-eq(it.body).flatten().join()
      },
    ),
  )
  show math.equation.where(block: true): it => if-not-paged(
    it,
    if sys.inputs.at("x-remove-html", default: none) != "true" {
      html.elem(
        "m1eqblock",
        html.frame(block(inset: 0.5em, it)),
      )
    } else {
      html.elem(
        "m1eqinline",
        process-math-eq(it.body).flatten().join(),
      )
    },
  )

  show linebreak: it => if-not-paged(it, md-linebreak)
  show figure: it => if-not-paged(it, md-figure(it.body, caption: it.caption))

  html.elem("m1document", body)
}
