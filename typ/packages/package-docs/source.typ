#import "mod.typ": *
#import "module.typ": package-title, package-setup, symbol-context
#import "sidebar.typ": package-layout
#import "/typ/packages/tinymist-index/lib.typ": file-by-uri, hover-markdown, index-source-tokens

#let source-line-numbers(source) = {
  [
    #for line in range(1, source.split("\n").len() + 1) {
      html.elem(
        "a",
        attrs: (id: source-line-id(line), class: "package-source-line-number", href: source-line-fragment(line)),
        str(line),
      )
    }
  ]
}

#let source-token-definition-dest(symbol-ctx, definition) = {
  if definition == none {
    return none
  }

  let uri = definition.at("targetUri", default: none)
  let target = definition.at("targetSelectionRange", default: none)
  if uri == none or target == none {
    return none
  }

  let file = file-by-uri(symbol-ctx.files, str(uri))
  if file == none or file.at("path", default: none) == none {
    return none
  }

  relative-path(symbol-ctx.at("path", default: none), str(file.path) + ".html") + source-line-fragment(target.start.line + 1)
}

#let source-token-data(symbol-ctx, source-path) = {
  let items = ()
  for token in index-source-tokens(symbol-ctx, source-path) {
    let hover = hover-markdown(token.at("hover", default: none))
    let href = source-token-definition-dest(symbol-ctx, token.at("definition", default: none))
    if hover != none or href != none {
      items.push((
        range: token.range,
        hover: hover,
        href: href,
      ))
    }
  }
  items
}

#let source-token-data-script(symbol-ctx, source-path) = {
  let tokens = source-token-data(symbol-ctx, source-path)

  [
    #html.elem(
      "script",
      attrs: (type: "application/json", class: "package-source-token-data"),
      json.encode(tokens),
    )
    #html.elem("script", attrs: (type: "module"), read("source-interactions.js"))
  ]
}

#let package-source-page(info, index, module-index: none, path: none, source-path: none, source: none) = {
  let title = package-title(info)

  package-setup(title)
  let symbol-ctx = symbol-context(info, index, bundle: true, path: path)
  let module-entry = info.modules.at(module-index)
  let module-info = module-entry.at(2)

  package-layout(info, symbol-ctx, active-module-index: module-index)[
    #let module-link = module-nav-dest(symbol-ctx, module-index, module-info)
    #html.elem("p", attrs: (class: "module-source-meta"), [
      #html.elem("a", attrs: (class: "module-source-link", href: module-link), "Module Docs")
    ])
    #labelled-heading(1, "Source: " + str(source-path))
    #html.elem("div", attrs: (class: "package-source-code", "data-source-path": str(source-path)), [
      #html.elem("div", attrs: (class: "package-source-body"), [
        #html.elem("pre", attrs: (class: "package-source-lines", "aria-hidden": "true"), source-line-numbers(source))
        #html.elem("div", attrs: (class: "package-source-scroll"), [
          #raw(source, lang: "typ", block: true)
        ])
      ])
      #source-token-data-script(symbol-ctx, source-path)
    ])
  ]
}

#let package-source-document(info, index, module-index: none, path: none, source-path: none, source: none) = {
  let title = package-title(info) + " - Source: " + str(source-path)

  document(path, title: title)[
    #package-source-page(
      info,
      index,
      module-index: module-index,
      path: path,
      source-path: source-path,
      source: source,
    )
  ]
}
