#let typ(body) = html.elem(
  "typ",
  {
    // distinguish parbreak from <p> tag
    show parbreak: it => html.elem("typParbreak", "")
    show linebreak: it => html.elem("typLinebreak", "")

    show strong: it => html.elem("typStrong", it.body)
    show emph: it => html.elem("typEmph", it.body)
    show highlight: it => html.elem("typHighlight", it.body)
    show strike: it => html.elem("typStrike", it.body)

    show raw.where(block: false): it => html.elem("typRawInline", "`" + it.text + "`")
    show raw.where(block: true): it => html.elem("typRawBlock", attrs: (lang: it.lang), it)
    show link: it => html.elem("typLink", attrs: (target: it.dest), it.body)
    // show label: it => html.elem("typLabel", it)
    show ref: it => html.elem("typRef", it)
    show heading: it => html.elem("typHeading", attrs: (level: str(it.level)), it)
    show outline: it => html.elem(
      "typOutline",
      it,
    )
    show outline.entry: it => html.elem(
      "typOutlineEntry",
      attrs: (level: str(it.level)),
      it.element,
    )
    // list
    // enum
    // term
    show quote: it => html.elem(
      "typQuote",
      attrs: (
        attribution: it.attribution,
      ),
      it.body,
    )
    show table: it => html.elem(
      "typTable",
      attrs: (
        columns: {
          let columns = it.columns
          if type(columns) == array {
            str(columns.len())
          } else if type(columns) == int {
            str(columns)
          } else {
            error("Invalid columns type")
          }
        },
      ),
      {
        it
          .children
          .map(cell => html.elem(
            "typTableCell",
            attrs: (
              colspan: str(cell.fields().at("colspan", default: 1)),
              rowspan: str(cell.fields().at("rowspan", default: 1)),
            ),
            cell,
          ))
          .join()
      },
    )
    show grid: it => html.elem(
      "typGrid",
      attrs: (
        columns: {
          let columns = it.columns
          if type(columns) == array {
            str(columns.len())
          } else if type(columns) == int {
            str(columns)
          } else {
            error("Invalid columns type")
          }
        },
      ),
      {
        it
          .children
          .map(cell => html.elem(
            "typGridCell",
            attrs: (
              colspan: str(cell.fields().at("colspan", default: 1)),
              rowspan: str(cell.fields().at("rowspan", default: 1)),
            ),
            cell,
          ))
          .join()
      },
    )

    // show math.equation.where(block: false): it => html.elem("typEquationInline", html.frame(it))
    // show math.equation.where(block: true): it => html.elem("typEquationBlock", html.frame(it))
    body
  },
)
