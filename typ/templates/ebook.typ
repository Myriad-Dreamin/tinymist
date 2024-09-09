#import "@preview/shiroa:0.1.1": *
#import "/typ/templates/page.typ": project, part-style

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
    heading(title)
  }

  locate(loc => {
    let inc = _resolve-inclusion-state.final(loc)
    external-book(spec: inc(spec))

    let mt = book-meta-state.final(loc)
    let styles = (inc: inc, part: part-style, chapter: it => it)

    if mt != none {
      mt.summary.map(it => visit-summary(it, styles)).sum()
    }
  })

  content
}