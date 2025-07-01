// This is important for shiroa to produce a responsive layout
// and multiple targets.
#import "@preview/shiroa:0.2.3": (
  get-page-width, html-support, is-html-target, is-pdf-target, is-web-target, plain-text, shiroa-sys-target, target,
  templates,
)
#import templates: *
#import html-support: *
#import "@preview/numbly:0.1.0": numbly
#import "theme.typ": *

#let web-theme = "starlight"
#let is-starlight-theme = web-theme == "starlight"

// Metadata
#let page-width = get-page-width()
#let is-html-target = is-html-target()
#let is-pdf-target = is-pdf-target()
#let is-web-target = is-web-target()
#let is-md-target = target == "md"
#let sys-is-html-target = ("target" in dictionary(std))

// Theme (Colors)
#let themes = theme-box-styles-from(toml("theme-style.toml"), read: it => read(it))
#let (
  default-theme: (
    style: theme-style,
    is-dark: is-dark-theme,
    is-light: is-light-theme,
    main-color: main-color,
    dash-color: dash-color,
    code-extra-colors: code-extra-colors,
  ),
) = themes;
#let (
  default-theme: default-theme,
) = themes;
#let theme-box = theme-box.with(themes: themes)

// Fonts
#let main-font = (
  "Charter",
  "Libertinus Serif",
  "Source Han Serif SC",
  // shiroa's embedded font
)
#let code-font = (
  "BlexMono Nerd Font Mono",
  // shiroa's embedded font
  "DejaVu Sans Mono",
)

#let part-counter = counter("shiroa-part-counter")

#let md-equation-rules(body) = {
  // equation setting
  show math.equation: it => theme-box(
    tag: if it.block { "p" } else { "span" },
    theme => {
      set text(fill: if theme.is-dark { gh-dark-fg } else { theme.main-color })
      html.frame(it)
    },
  )

  body
}

/// The project function defines how your document looks.
/// It takes your content and some metadata and formats it.
/// Go ahead and customize it to your liking!
#let project(title: "Tinymist Docs", authors: (), kind: "page", description: none, body) = {
  // set basic document metadata
  set document(
    author: authors,
    title: title,
  ) if not is-pdf-target and not is-md-target

  // todo dirty hack to check is main
  let is-main = title == "Tinymist Documentation"

  // set web/pdf page properties
  set page(
    numbering: none,
    number-align: center,
    width: page-width,
  ) if not (sys-is-html-target or is-html-target)
  set page(numbering: "1") if (not sys-is-html-target and is-pdf-target) and not is-main and kind == "page"

  // remove margins for web target
  set page(
    margin: (
      // reserved beautiful top margin
      top: 20pt,
      // reserved for our heading style.
      // If you apply a different heading style, you may remove it.
      left: 20pt,
      // Typst is setting the page's bottom to the baseline of the last line of text. So bad :(.
      bottom: 0.5em,
      // remove rest margins.
      rest: 0pt,
    ),
    height: auto,
  ) if is-web-target and not is-html-target

  show: if is-html-target {
    import "@preview/shiroa-starlight:0.2.3": starlight

    let description = if description != none { description } else {
      let desc = plain-text(body, limit: 512).trim()
      if desc.len() > 512 {
        desc = desc.slice(0, 512) + "..."
      }
      desc
    }

    starlight.with(
      include "/docs/tinymist/book.typ",
      title: title,
      description: description,
      github-link: "https://github.com/Myriad-Dreamin/tinymist",
    )
  } else {
    it => it
  }

  // Set main text
  set text(
    font: main-font,
    size: main-size,
    fill: main-color,
    lang: "en",
  )

  show: if is-md-target {
    it => it
  } else {
    markup-rules.with(web-theme: web-theme, dash-color: dash-color)
  }
  show: if is-md-target {
    it => it
  } else {
    it => {
      set heading(numbering: "1.") if is-pdf-target and not is-main
      it
    }
  }

  show: if is-md-target {
    md-equation-rules
  } else {
    equation-rules.with(theme-box: theme-box)
  }

  show: if is-md-target {
    it => it
  } else {
    code-block-rules.with(
      zebraw: "@preview/zebraw:0.5.5",
      themes: themes,
      code-font: code-font,
    )
  }

  if not is-md-target {
    context if shiroa-sys-target() == "html" {
      show raw: it => html.elem("style", it.text)
      ```css
      .pseudo-image svg {
        width: 100%
      }
      ```
    }
  }

  show <typst-raw-func>: it => {
    it.lines.at(0).body.children.slice(0, -2).join()
  }

  if kind == "page" and is-pdf-target and not is-main {
    text(size: 32pt, heading(level: 1, numbering: none, title))
  }

  // Main body.
  set par(justify: true)

  body

  // Put your custom CSS here.
  add-styles(
    ```css
    .site-title {
      font-size: 1.2rem;
      font-weight: 600;
      font-style: italic;
    }
    ```,
  )
}

#let part-style(it) = {
  set text(size: heading-sizes.at(0))
  set text(weight: "bold")
  set text(fill: main-color)
  part-counter.step()

  context heading(numbering: none, [Part #part-counter.display(numbly("{1}. "))#it])
  counter(heading).update(0)
}
