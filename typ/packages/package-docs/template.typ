// This is important for shiroa to produce a responsive layout
// and multiple targets.
#import "@preview/shiroa:0.2.2": (
  get-page-width,
  target,
  is-web-target,
  is-pdf-target,
  is-html-target,
  plain-text,
  shiroa-sys-target,
  templates,
)
#import templates: *
#import "@preview/numbly:0.1.0": numbly
#import "@preview/zebraw:0.5.2": zebraw-init, zebraw
#import "theme.typ": *

// Metadata
#let page-width = get-page-width()
#let is-html-target = is-html-target()
#let is-pdf-target = is-pdf-target()
#let is-web-target = is-web-target()
#let is-md-target = target == "md"
#let sys-is-html-target = ("target" in dictionary(std))

#let part-counter = counter("shiroa-part-counter")
/// Creates an embedded block typst frame.
#let div-frame(content, attrs: (:)) = html.elem("div", html.frame(content), attrs: attrs)
#let span-frame(content, attrs: (:)) = html.elem("span", html.frame(content), attrs: attrs)

// Theme (Colors)
#let (
  style: theme-style,
  is-dark: is-dark-theme,
  is-light: is-light-theme,
  main-color: main-color,
  dash-color: dash-color,
  code-extra-colors: code-extra-colors,
) = book-theme-from(toml("theme-style.toml"), xml: it => xml(it))
#let gh-dark-fg = rgb("#f0f6fc")

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

// Sizes
#let main-size = if is-web-target {
  16pt
} else {
  10.5pt
}
// ,
#let heading-sizes = (22pt, 18pt, 14pt, 12pt, main-size)
#let list-indent = 0.5em

#let raw-rules(body) = {
  let init-with-theme((code-extra-colors, is-dark)) = if is-dark {
    zebraw-init.with(
      // should vary by theme
      background-color: if code-extra-colors.bg != none {
        (code-extra-colors.bg, code-extra-colors.bg)
      },
      highlight-color: rgb("#3d59a1"),
      comment-color: rgb("#394b70"),
      lang-color: rgb("#3d59a1"),
      lang: false,
      numbering: false,
    )
  } else {
    zebraw-init.with(
      // should vary by theme
      background-color: if code-extra-colors.bg != none {
        (code-extra-colors.bg, code-extra-colors.bg)
      },
      lang: false,
      numbering: false,
    )
  }

  /// HTML code block supported by zebraw.
  show: init-with-theme((code-extra-colors, is-dark-theme))

  // code block setting
  set raw(theme: theme-style.code-theme) if theme-style.code-theme.len() > 0
  show raw: set text(font: code-font)
  show raw.where(block: true): it => context if shiroa-sys-target() == "paged" {
    rect(
      width: 100%,
      inset: (x: 4pt, y: 5pt),
      radius: 4pt,
      fill: code-extra-colors.bg,
      [
        #set text(fill: code-extra-colors.fg) if code-extra-colors.fg != none
        #set par(justify: false)
        // #place(right, text(luma(110), it.lang))
        #it
      ],
    )
  } else {
    set text(fill: code-extra-colors.fg) if code-extra-colors.fg != none
    set par(justify: false)
    zebraw(
      block-width: 100%,
      // line-width: 100%,
      wrap: false,
      it,
    )
  }

  body
}

#let equation-rules(body) = {
  // equation setting
  show math.equation: set text(weight: 400)
  show math.equation.where(block: true): it => context if shiroa-sys-target() == "html" {
    div-frame(attrs: ("style": "display: flex; justify-content: center; overflow-x: auto;"), it)
  } else {
    it
  }
  show math.equation.where(block: false): it => context if shiroa-sys-target() == "html" {
    span-frame(attrs: ("style": "overflow-x: auto;"), it)
  } else {
    it
  }

  body
}

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
#let project(title: "Tinymist Docs", authors: (), kind: "page", body) = {
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

  // Set main text
  set text(
    font: main-font,
    size: main-size,
    fill: main-color,
    lang: "en",
  )

  // Set main spacing
  set enum(
    indent: list-indent * 0.618,
    body-indent: list-indent,
  )
  set list(
    indent: list-indent * 0.618,
    body-indent: list-indent,
  )
  set par(leading: 0.7em)
  set block(spacing: 0.7em * 1.5)

  // Set text, spacing for headings
  // Render a dash to hint headings instead of bolding it as well if it's for web.
  show heading: set text(weight: "regular") if is-web-target
  show heading: it => {
    set text(size: heading-sizes.at(it.level))

    block(
      spacing: 0.7em * 1.5 * 1.2,
      below: 0.7em * 1.2,
      {
        // if is-web-target {
        //   heading-hash(it, hash-color: dash-color)
        // }

        it
      },
    )
  }
  set heading(
    numbering: (..numbers) => context {
      if part-counter.get().at(0) > 0 {
        numbering("1.", ..part-counter.get(), ..numbers)
      } else {
        h(-0.3em)
      }
    },
  ) if is-pdf-target

  // link setting
  show link: set text(fill: dash-color)

  show: if is-md-target {
    md-equation-rules
  } else {
    equation-rules
  }

  show: if is-md-target {
    it => it
  } else {
    raw-rules
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
    heading(level: 1, numbering: none, text(size: 32pt, title))
  }

  // Main body.
  set par(justify: true)

  body
}

#let part-style(it) = {
  set text(size: heading-sizes.at(0))
  set text(weight: "bold")
  set text(fill: main-color)
  part-counter.step()

  context heading(numbering: none, [Part #part-counter.display(numbly("{1}. "))#it])
  counter(heading).update(0)
}
