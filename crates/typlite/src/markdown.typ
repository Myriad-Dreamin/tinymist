#let bool-str(x) = {
  if x {
    "true"
  } else {
    "false"
  }
}

#let md-parbreak = html.elem("m1parbreak", "")

#let md-grid(it) = html.elem(
  "m1grid",
  table(columns: it.columns, ..it
      .children
      .map(child => {
        {
          let func = child.func()
          if func == grid.cell {
            table.cell(
              child.body,
            )
          } else if func == grid.header {
            table.header(..child.children.map(it => table.cell(
              it.body,
            )))
          } else if func == grid.footer {
            table.footer(..child.children.map(it => table.cell(
              it.body,
            )))
          }
        }
      })
      .flatten()),
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
  show parbreak: it => if-not-paged(it, md-parbreak)
  show grid: it => if-not-paged(it, md-grid(it))
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

  html.elem("m1document", body)
}
