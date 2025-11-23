#let elem(kind, ..args) = html.elem(
  "m1" + kind,
  attrs: args
    .named()
    .pairs()
    .map(((k, v)) => (
      k,
      {
        if type(v) == bool {
          if (v) { "true" } else { "" }
        } else if v == none {
          ""
        } else if v == auto {
          "auto"
        } else if type(v) == bytes {
          // from digestify.typ
          let bytes-to-hex(bytes, upper: false) = {
            let res = array(bytes)
              .map(x => {
                let s = str(x, base: 16)
                return "0" * (2 - s.len()) + s
              })
              .join()

            if upper {
              return std.upper(res)
            } else {
              return res
            }
          }
          bytes-to-hex(v)
        } else { str(v) }
      },
    ))
    .to-dict(),
  { args.pos().join() },
)

#let rewrap(tag) = {
  return x => html.span(elem(tag, x.body))
}

#let attributed(tag, attrs) = {
  return x => elem(tag, ..attrs.map(attr => (attr, x.fields().at(attr))).to-dict())
}

#let attributed-rewrap(tag, attrs) = {
  return x => elem(tag, x.body, ..attrs.map(attr => (attr, x.fields().at(attr))).to-dict())
}

#let const-func(res) = {
  return (..x) => res
}

#let rules = (
  ..(
    strong,
    emph,
    footnote,
    underline,
    strike,
    overline,
    sub,
    super,
    highlight,
    smallcaps,
  ).map(x => (
    x,
    rewrap(repr(x)),
  )),
  ..(
    linebreak,
    pagebreak,
    parbreak,
  ).map(x => (x, const-func(elem(repr(x))))),
  ..(
    (heading, ("level", "numbering")),
    (quote, ("block",)),
  ).map(x => (
    x.first(),
    attributed-rewrap(repr(x.first()), x.last()),
  )),
  ..(
    (ref, ("target", "supplement")),
    (cite, ("key", "supplement")),
    (image, ("source", "alt", "width", "height")),
  ).map(x => (
    x.first(),
    attributed(repr(x.first()), x.last()),
  )),
  (
    raw,
    it => if it.block { elem("raw", text: it.text, block: it.block, lang: it.lang) } else {
      html.span(elem("raw", text: it.text, block: it.block, lang: it.lang))
    },
  ),
  (list, it => elem("list", tight: it.tight, ..it.children.map(it => elem("item", it.body)))),
  (
    enum,
    it => elem("enum", reversed: it.reversed, start: it.start, tight: it.tight, ..it.children.map(
      it => elem("item", it.body),
    )),
  ),
  (
    terms,
    it => elem("terms", tight: it.tight, ..it.children.map(
      it => elem("item", elem("term", it.term), elem("description", it.description)),
    )),
  ),
  (
    link,
    it => {
      let kind = if type(it.dest) == str { "url" } else { repr(type(it.dest)) }
      elem(
        "link",
        elem("dest", kind: kind, dest: if type(it.dest) == location { "location" } else { str(it.dest) }),
        elem("body", it.body),
      )
    },
  ),
  (
    table,
    it => elem(
      "table",
      columns: if type(it.columns) == array { it.columns.len() } else { it.columns },
      ..it.children.map(
        child => if type(child) == table.cell {
          elem(
            "cell",
            child.body,
          )
        } else if type(child) == table.header {
          elem(
            "header",
            ..child.cells.map(
              cell => elem(
                "cell",
                cell.body,
              ),
            ),
          )
        } else if type(child) == table.footer {
          elem(
            "footer",
            ..child.cells.map(
              cell => elem(
                "cell",
                cell.body,
              ),
            ),
          )
        },
      ),
    ),
  ),
  (
    figure,
    it => elem("figure", elem("body", it.body), elem("caption", it.caption), kind: repr(
      it.kind,
    )),
  ),
  (
    bibliography,
    it => elem(
      "bibliography",
      // TODO: this place should not use join directly
      elem("sources", it.sources.join()),
      full: it.full,
      title: it.title,
      style: it.style,
    ),
  ),
  (math.equation, it => elem("equation", html.frame(it))),
  it => elem("document", it),
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

#let md-doc = rules.fold(
  it => it,
  (acc, sr) => it => context {
    if type(sr) == array {
      let (selector, rule) = sr
      if target() != "html" {
        return it
      }
      show selector: rule
      acc(it)
    } else {
      show: sr
      acc(it)
    }
  },
)
