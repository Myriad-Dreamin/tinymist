#import "mod.typ": *
#import "function.typ": function-symbol-page
#import "sidebar.typ": package-layout
#import "symbols.typ": symbol-doc, symbol-reference-list
#import "/typ/packages/tinymist-index/lib.typ": index-public-symbols

#let module-doc(info: none, name: none, symbol-ctx, m, module-index: none, references-only: false) = {
  let m = analyze-module(m)

  if info.prefix.len() > 0 {
    module-divider
    labelled-heading(1, module-heading-title(info))
  } else {
    labelled-heading(1, module-heading-title(info))
  }

  let source-dest = module-source-dest(symbol-ctx, info)
  if source-dest != none {
    html.elem("p", attrs: (class: "module-source-meta"), [
      #html.elem("a", attrs: (class: "module-source-link", href: source-dest), "Source")
    ])
  }

  let symbol-ctx = (
    in-module: info.prefix,
    module-index: module-index,
    module-info: info,
    public-symbols: index-public-symbols(symbol-ctx, info),
    ..symbol-ctx,
  )

  if m.modules.len() > 0 {
    labelled-heading(2, "Modules")
    if references-only {
      symbol-reference-list(symbol-ctx, m.modules)
    } else {
      for child in m.modules {
        symbol-doc(symbol-ctx, child)
      }
    }
  }
  if m.constants.len() > 0 {
    labelled-heading(2, "Constants")
    if references-only {
      symbol-reference-list(symbol-ctx, m.constants)
    } else {
      for child in m.constants {
        symbol-doc(symbol-ctx, child)
      }
    }
  }
  if m.functions.len() > 0 {
    labelled-heading(2, "Functions")
    if references-only {
      symbol-reference-list(symbol-ctx, m.functions)
    } else {
      for child in m.functions {
        symbol-doc(symbol-ctx, child)
      }
    }
  }
  if m.unknowns.len() > 0 {
    [== Unknowns]
    for child in m.unknowns {
      symbol-doc(symbol-ctx, child)
    }
  }
}
#let package-title(info) = {
  "@" + info.meta.namespace + "/" + info.meta.name + " " + info.meta.version
}

#let package-setup(title) = {
  show: project.with(title: title)
  html.elem("style", read("global.css"))
}

#let package-header(info, title) = {
  strong[
    This documentation is generated locally. Please submit issues to #link("https://github.com/Myriad-Dreamin/tinymist")[tinymist] if you see incorrect information in it.
  ]

  html.elem("h1", attrs: (id: "package-doc-title"), title)

  let repo = info.meta.manifest.package.at("repository", default: none)
  if repo != none {
    let repo_link = html.elem("a", attrs: (href: repo, class: "package-repo-link"), "Repository")
    html.elem("p", repo_link)
  }

  let description = info.meta.manifest.package.at("description", default: none)
  if description != none {
    description
  }
}

#let symbol-context(info, index, bundle: false, path: none) = {
  (
    index: index,
    bundle: bundle,
    path: path,
    ..analyze-package(info),
  )
}

#let package-module-page(info, index, module-index: none, show-header: true, bundle: false, path: none) = {
  let title = "@" + info.meta.namespace + "/" + info.meta.name + " " + info.meta.version

  package-setup(title)
  let symbol-ctx = symbol-context(info, index, bundle: bundle, path: path)
  let module-entry = info.modules.at(module-index)

  if show-header {
    package-layout(info, symbol-ctx, active-module-index: module-index)[
      #package-header(info, title)
      #module-doc(
        info: module-entry.at(2),
        name: module-entry.at(0),
        symbol-ctx,
        module-entry.at(1),
        module-index: module-index,
        references-only: bundle,
      )
    ]
  } else {
    package-layout(info, symbol-ctx, active-module-index: module-index)[
      #module-doc(
        info: module-entry.at(2),
        name: module-entry.at(0),
        symbol-ctx,
        module-entry.at(1),
        module-index: module-index,
        references-only: bundle,
      )
    ]
  }
}

#let package-module-document(info, index, module-index: none, path: none) = {
  let module-entry = info.modules.at(module-index)
  let info-title = package-title(info)
  let title = info-title + " - " + module-title(module-entry.at(2))

  document(path, title: title)[
    #package-module-page(
      info,
      index,
      module-index: module-index,
      show-header: module-index == 0,
      bundle: true,
      path: path,
    )
  ]
}

#let package-module-symbol-page(info, index, module-index: none, section: none, symbol-index: none, path: none) = {
  let title = package-title(info)

  package-setup(title)
  let symbol-ctx = symbol-context(info, index, bundle: true, path: path)
  let module-entry = info.modules.at(module-index)
  let module-info = module-entry.at(2)
  let m = analyze-module(module-entry.at(1))
  let items = m.at(section, default: ())
  let child = items.at(symbol-index)

  package-layout(
    info,
    symbol-ctx,
    active-module-index: module-index,
    active-section: section,
    active-symbol-index: symbol-index,
  )[
    #let module-link = module-nav-dest(symbol-ctx, module-index, module-info)
    #html.elem("p", attrs: (class: "module-source-meta"), [
      #html.elem("a", attrs: (class: "module-source-link", href: module-link), "Module Docs")
    ])
    #let symbol-ctx = (
      in-module: module-info.prefix,
      module-index: module-index,
      module-info: module-info,
      public-symbols: index-public-symbols(symbol-ctx, module-info),
      ..symbol-ctx,
    )
    #if child.kind == "function" {
      function-symbol-page(symbol-ctx, child)
    } else {
      labelled-heading(1, module-heading-title(module-info))
      labelled-heading(2, section-title(section))
      symbol-doc(symbol-ctx, child)
    }
  ]
}

#let package-module-symbol-document(info, index, module-index: none, section: none, symbol-index: none, path: none) = {
  let module-entry = info.modules.at(module-index)
  let m = analyze-module(module-entry.at(1))
  let child = m.at(section, default: ()).at(symbol-index)
  let info-title = package-title(info)
  let title = info-title + " - " + module-title(module-entry.at(2)) + " - " + child.name

  document(path, title: title)[
    #package-module-symbol-page(
      info,
      index,
      module-index: module-index,
      section: section,
      symbol-index: symbol-index,
      path: path,
    )
  ]
}
