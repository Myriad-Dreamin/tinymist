#let x-target = sys.inputs.at("x-target", default: "pdf")
#let show-if(cond, func) = body => if cond { func(body) } else { body }
#show grid: show-if(x-target == "md", it => {
  let columns = it.columns.len()
  html.elem("table", attrs: (x: "test"), {
    for row in it.children.chunks(columns) {
      html.elem("tr", {
        row.map(cell => html.elem("td", cell)).join()
      })
    }
  })
})

#show raw.where(block: true): show-if(x-target == "md", it => {
  html.elem("m1verbatim", attrs: (src: "````\n" + it.text + "\n````"))
})

= Reproducer document

== Example

#let src = {
  ````typ

  == Under the Greenwood Tree
  by Shakespeare.

  #some-function(```
  Under the greenwood tree #emoji.tree
  Who loves to lie with me,
  ...
  ```)
  ````
}

Just the source (ok)

#src

Source in grid
// HTML rendering not implemented for Custom Node: typlite::common::VerbatimNode

#grid(
  columns: 2,
  [Placeholder for \ separately rendered SVG], src,
)
