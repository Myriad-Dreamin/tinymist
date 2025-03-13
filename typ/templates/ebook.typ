#import "@preview/shiroa:0.2.0": *
#import "/typ/templates/page.typ": project, part-style, heading-sizes, main-color
#import "tinymist-version.typ": tinymist-package

#let _page-project = project

#let _resolve-inclusion-state = state("_resolve-inclusion", none)

#let resolve-inclusion(inc) = _resolve-inclusion-state.update(it => inc)

#let project(title: "", authors: (), spec: "", content) = {
  // Set document metadata early
  set document(
    author: authors,
    title: title,
  )

  // Inherit from gh-pages
  show: _page-project

  if title != "" {
    set text(size: heading-sizes.at(1))
    set text(weight: "bold")

    // if kind == "page" and is-pdf-target and not is-main {
    //   [= #title]
    // }
    title
    v(1em)
  }
  [
    #tinymist-package.description

    Visit tinymist repository: #link(tinymist-package.repository)[main branch, ] or #link({
      tinymist-package.repository
      "/tree/v"
      tinymist-package.version
    })[v#tinymist-package.version.]
  ]

  {
    // inherit from page setting
    show: _page-project.with(title: none, kind: "preface")

    include "/typ/templates/license.typ"

    let outline-numbering-base = numbering.with("1.")
    let outline-numbering(a0, ..args) = if a0 > 0 {
      h(1em * args.pos().len())
      outline-numbering-base(a0, ..args) + [ ]
    }

    let outline-counter = counter("outline-counter")
    show outline.entry: it => {
      let has-part = if it.body().func() != none and "children" in it.body().fields() {
        for ch in it.body().children {
          if "text" in ch.fields() and ch.text.contains("Part") {
            ch.text
          }
        }
      }

      // set link(main-color)
      show link: set text(fill: main-color)

      if has-part == none {
        outline-counter.step(level: it.level + 1)
        layout(shape => {
          context {
            let lnk = link(it.element.location(), [#outline-counter.display(outline-numbering) #it.element.body])
            let r = repeat([.])
            let page-no = str(it.element.location().page())
            let q = measure(lnk + page-no)
            lnk
            box(width: shape.width - q.width, inset: (x: 0.25em), r)
            page-no
          }
        })
      } else {
        outline-counter.step(level: 1)
        block(link(it.element.location(), it.element.body))
      }
    }

    set outline.entry(fill: repeat[.])
    outline(depth: 1)
  }

  context {
    let inc = _resolve-inclusion-state.final()
    external-book(spec: inc(spec))

    let mt = book-meta-state.final()
    let styles = (inc: inc, part: part-style, chapter: it => it)

    if mt != none {
      mt.summary.map(it => visit-summary(it, styles)).sum()
    }
  }

  content
}
