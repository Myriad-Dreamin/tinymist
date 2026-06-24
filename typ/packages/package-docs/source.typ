#import "mod.typ": *
#import "module.typ": package-title, package-setup, symbol-context
#import "sidebar.typ": package-layout

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

#let package-source-page(info, module-index: none, path: none, source-path: none, source: none) = {
  let title = package-title(info)

  package-setup(title)
  let symbol-ctx = symbol-context(info, none, bundle: true, path: path)
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
    ])
  ]
}

#let package-source-document(info, module-index: none, path: none, source-path: none, source: none) = {
  let title = package-title(info) + " - Source: " + str(source-path)

  document(path, title: title)[
    #package-source-page(
      info,
      module-index: module-index,
      path: path,
      source-path: source-path,
      source: source,
    )
  ]
}
