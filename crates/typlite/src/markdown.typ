#let md-parbreak = html.elem("typParbreak", "")
#let md-linebreak = html.elem("typLinebreak", "")
#let md-strong = html.elem.with("typStrong")
#let md-emph = html.elem.with("typEmph")
#let md-highlight = html.elem.with("typHighlight")
#let md-strike = html.elem.with("typStrike")
#let md-raw(lang: none, block: false, text) = html.elem(
  "typRaw",
  attrs: (lang: lang, block: block, text: text),
  "",
)
#let md-link(dest: none, body) = html.elem(
  "typLink",
  attrs: (dest: dest),
  body,
)
#let md-label(dest: none, body) = html.elem(
  "typLabel",
  attrs: (dest: dest),
  body,
)
#let md-ref(body) = html.elem(
  "typRef",
  body,
)
#let md-heading(level: int, body) = html.elem(
  "typHeading",
  attrs: (level: str(level)),
  body,
)
#let md-outline = html.elem.with("typOutline")
#let md-outline-entry(level: int, body) = html.elem(
  "typOutlineEntry",
  attrs: (level: str(level)),
  body,
)
#let md-quote(attribution: none, body) = html.elem(
  "typQuote",
  attrs: (attribution: attribution),
  body,
)
#let md-table(columns: auto, ..children) = html.elem(
  "typTable",
  attrs: (
    columns: str({
      if type(columns) == array {
        str(columns.len())
      } else if type(columns) == int {
        str(columns)
      } else if type(columns) == auto {
        str(children.len())
      } else {
        error("Invalid columns type")
      }
    }),
  ),
  children
    .pos()
    .map(cell => html.elem(
      "typTableCell",
      attrs: (
        colspan: str(cell.fields().at("colspan", default: 1)),
        rowspan: str(cell.fields().at("rowspan", default: 1)),
      ),
      cell,
    ))
    .join(),
)
#let md-grid(columns: auto, ..children) = html.elem(
  "typGrid",
  attrs: (
    columns: str({
      if type(columns) == array {
        str(columns.len())
      } else if type(columns) == int {
        str(columns)
      } else if type(columns) == auto {
        str(children.len())
      } else {
        error("Invalid columns type")
      }
    }),
  ),
  children
    .pos()
    .map(cell => html.elem(
      "typGridCell",
      attrs: (
        colspan: str(cell.fields().at("colspan", default: 1)),
        rowspan: str(cell.fields().at("rowspan", default: 1)),
      ),
      cell,
    ))
    .join(),
)

#let md-doc(body) = {
  // distinguish parbreak from <p> tag
  show parbreak: md-parbreak
  show linebreak: md-linebreak
  show strong: md-strong
  show emph: md-emph
  show highlight: md-highlight
  show strike: md-strike

  show raw: it => md-raw(lang: it.lang, block: it.block, it.text)
  show link: it => md-link(dest: it.dest, it.body)
  // show label: it => html.elem("typLabel", it)
  show ref: it => md-ref(it)

  show heading: it => md-heading(level: it.level, it.body)
  show outline: md-outline
  show outline.entry: it => md-outline-entry(level: it.level, it.element)
  show quote: it => html.elem(
    "typQuote",
    attrs: (
      attribution: it.attribution,
    ),
    it.body,
  )
  show quote: it => md-quote(attribution: it.attribution, it.body)
  show table: it => md-table(
    columns: it.columns,
    children: it.children,
  )
  show grid: it => md-grid(
    columns: it.columns,
    children: it.children,
  )

  show math.equation.where(block: false): it => html.elem("typEquationInline", html.frame(it))
  show math.equation.where(block: true): it => html.elem("typEquationBlock", html.frame(it))


  html.elem("typDocument", body)
}
