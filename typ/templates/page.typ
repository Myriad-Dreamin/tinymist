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

// Sizes
#let main-size = if is-web-target {
  16pt
} else {
  10.5pt
}
#let heading-sizes = if is-web-target {
  (2, 1.5, 1.17, 1, 0.83).map(it => it * main-size)
} else {
  (26pt, 22pt, 14pt, 12pt, main-size)
}
#let list-indent = 0.5em

#let markup-rules(
  body,
  dash-color: none,
  web-theme: "starlight",
  main-size: main-size,
  heading-sizes: heading-sizes,
  list-indent: list-indent,
  starlight: "@preview/shiroa-starlight:0.2.3",
) = {
  assert(dash-color != none, message: "dash-color must be set")

  let is-starlight-theme = web-theme == "starlight"
  let in-heading = state("shiroa:in-heading", false)

  let mdbook-heading-rule(it) = {
    let it = {
      set text(size: heading-sizes.at(it.level))
      if is-web-target {
        heading-hash(it, hash-color: dash-color)
      }

      in-heading.update(true)
      it
      in-heading.update(false)
    }

    block(
      spacing: 0.7em * 1.5 * 1.2,
      below: 0.7em * 1.2,
      it,
    )
  }

  let starlight-heading-rule(it) = context if shiroa-sys-target() == "html" {
    import starlight: builtin-icon

    in-heading.update(true)
    html.elem("div", attrs: (class: "sl-heading-wrapper level-h" + str(it.level + 1)))[
      #it
      #html.elem(
        "h" + str(it.level + 1),
        attrs: (class: "sl-heading-anchor not-content", role: "presentation"),
        static-heading-link(it, body: builtin-icon("anchor"), canonical: true),
      )
    ]
    in-heading.update(false)
  } else {
    mdbook-heading-rule(it)
  }


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
  // todo: add me back in mdbook theme!!!
  show heading: if is-starlight-theme {
    starlight-heading-rule
  } else {
    mdbook-heading-rule
  }

  // link setting
  show link: set text(fill: dash-color)

  body
}

#let equation-rules(
  body,
  web-theme: "starlight",
  theme-box: none,
) = {
  // import "supports-html.typ": add-styles
  let is-starlight-theme = web-theme == "starlight"
  let in-heading = state("shiroa:in-heading", false)

  /// Creates an embedded block typst frame.
  let div-frame(content, attrs: (:), tag: "div") = html.elem(tag, html.frame(content), attrs: attrs)
  let span-frame = div-frame.with(tag: "span")
  let p-frame = div-frame.with(tag: "p")


  let get-main-color(theme) = {
    if is-starlight-theme and theme.is-dark and in-heading.get() {
      white
    } else {
      theme.main-color
    }
  }

  show math.equation: set text(weight: 400)
  show math.equation.where(block: true): it => context if shiroa-sys-target() == "html" {
    theme-box(tag: "div", theme => {
      set text(fill: get-main-color(theme))
      p-frame(attrs: ("class": "block-equation", "role": "math"), it)
    })
  } else {
    it
  }
  show math.equation.where(block: false): it => context if shiroa-sys-target() == "html" {
    theme-box(tag: "span", theme => {
      set text(fill: get-main-color(theme))
      span-frame(attrs: (class: "inline-equation", "role": "math"), it)
    })
  } else {
    it
  }

  // add-styles(
  //   ```css
  //   .inline-equation {
  //     display: inline-block;
  //     width: fit-content;
  //   }
  //   .block-equation {
  //     display: grid;
  //     place-items: center;
  //     overflow-x: auto;
  //   }
  //   ```,
  // )
  body
}

#let code-block-rules(
  body,
  web-theme: "starlight",
  code-font: none,
  themes: none,
  zebraw: "@preview/zebraw:0.5.5",
) = {
  import zebraw: zebraw, zebraw-init

  let with-raw-theme = (theme, it) => {
    if theme.len() > 0 {
      raw(
        align: it.align,
        tab-size: 2,
        block: it.block,
        lang: it.lang,
        syntaxes: it.syntaxes,
        theme: theme,
        it.text,
      )
    } else {
      raw(
        align: it.align,
        tab-size: 2,
        block: it.block,
        lang: it.lang,
        syntaxes: it.syntaxes,
        theme: auto,
        it.text,
      )
    }
  }

  let (
    default-theme: (
      style: theme-style,
      is-dark: is-dark-theme,
      is-light: is-light-theme,
      main-color: main-color,
      dash-color: dash-color,
      code-extra-colors: code-extra-colors,
    ),
  ) = themes
  let (
    default-theme: default-theme,
  ) = themes
  let theme-box = theme-box.with(themes: themes)

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
  show: init-with-theme(default-theme)
  set raw(tab-size: 114)

  let in-mk-raw = state("shiroa:in-mk-raw", false)
  let mk-raw(
    it,
    tag: "div",
    inline: false,
  ) = {
    theme-box(tag: tag, theme => {
      show: init-with-theme(theme)
      let code-extra-colors = theme.code-extra-colors
      let use-fg = not inline and code-extra-colors.fg != none
      set text(fill: code-extra-colors.fg) if use-fg
      set text(fill: if theme.is-dark { rgb("dfdfd6") } else { black }) if not use-fg
      set par(justify: false)
      zebraw(
        block-width: 100%,
        // line-width: 100%,
        wrap: false,
        with-raw-theme(theme.style.code-theme, it),
      )
    })
  }

  show raw: set text(font: code-font) if code-font != none
  show raw.where(block: false, tab-size: 114): it => context if shiroa-sys-target() == "paged" {
    it
  } else {
    mk-raw(it, tag: "span", inline: true)
  }
  show raw.where(block: true, tab-size: 114): it => context if shiroa-sys-target() == "paged" {
    rect(width: 100%, inset: (x: 4pt, y: 5pt), radius: 4pt, fill: code-extra-colors.bg, {
      set text(fill: code-extra-colors.fg) if code-extra-colors.fg != none
      set par(justify: false)
      with-raw-theme(theme-style.code-theme, it)
    })
  } else {
    mk-raw(it)
  }
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

  // todo: ...
  // Put your custom CSS here.
  // add-styles(
  //   ```css
  //   .site-title {
  //     font-size: 1.2rem;
  //     font-weight: 600;
  //     font-style: italic;
  //   }
  //   ```,

  // Put your custom CSS here.
  context if shiroa-sys-target() == "html" {
    html.elem(
      "style",
      ```css
      .inline-equation {
        display: inline-block;
        width: fit-content;
      }
      .block-equation {
        display: grid;
        place-items: center;
        overflow-x: auto;
      }
      .site-title {
        font-size: 1.2rem;
        font-weight: 600;
        font-style: italic;
      }
      ```.text,
    )
  }
}

#let part-style(it) = {
  set text(size: heading-sizes.at(0))
  set text(weight: "bold")
  set text(fill: main-color)
  part-counter.step()

  context heading(numbering: none, [Part #part-counter.display(numbly("{1}. "))#it])
  counter(heading).update(0)
}
