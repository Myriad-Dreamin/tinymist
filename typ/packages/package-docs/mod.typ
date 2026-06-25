#import "@preview/cmarker:0.1.6": render
// re-export page template
#import "template.typ": project

#let module-divider = html.elem("hr", attrs: (class: "module-divider"));
#show link: it => if type(it.dest) == label {
  html.elem("a", attrs: (href: "#" + str(it.dest), class: "symbol-link"), it.body)
} else {
  it
}
#let heading-label(name) = {
  let it = name.replace(regex("[\s\:]"), "-").replace(regex("[.()]"), "").replace(regex("-+"), "-").replace("M", "m")
  label(it)
}
#let labelled-heading(depth, it, dest: none) = {
  let body = if dest == none {
    it
  } else {
    html.elem("a", attrs: (href: dest, class: "symbol-link"), it)
  }
  heading(depth: depth, html.elem("span", attrs: (id: str(heading-label(it))), body))
}
#let markdown-docs = render.with(
  scope: (
    image: (src, alt: none) => {
      html.elem("img", attrs: (src: src, alt: alt, class: "code-image"))
    },
  ),
)

#let display-package-spec(pkg-spec) = {
  "@"
  pkg-spec.meta.namespace
  "/"
  pkg-spec.meta.name
  ":"
  pkg-spec.meta.version
}

#let span = html.elem.with("span")
#let code = html.elem.with("code")
#let keyword = code.with(attrs: (class: "code-kw"))
#let builtin-ty = code.with(attrs: (class: "type-builtin"))

#let path-parts(path) = {
  let parts = ()
  for part in str(path).split("/") {
    if part != "" and part != "." {
      parts.push(part)
    }
  }
  parts
}

#let relative-path(from, to) = {
  if from == none or to == none {
    return to
  }

  let from-parts = path-parts(from)
  if from-parts.len() > 0 {
    from-parts = from-parts.slice(0, -1)
  }

  let to-parts = path-parts(to)
  while (
    from-parts.len() > 0 and
    to-parts.len() > 0 and
    from-parts.at(0) == to-parts.at(0)
  ) {
    from-parts = from-parts.slice(1)
    to-parts = to-parts.slice(1)
  }

  let rel = ()
  for part in from-parts {
    rel.push("..")
  }
  for part in to-parts {
    rel.push(part)
  }

  if rel.len() == 0 {
    "."
  } else {
    rel.join("/")
  }
}

#let symbol-dest(symbol-ctx, info) = {
  if symbol-ctx.at("bundle", default: false) and info.at("bundle_link", default: none) != none {
    return relative-path(symbol-ctx.at("path", default: none), info.bundle_link)
  }
  if info.at("symbol_link", default: none) != none {
    return info.symbol_link
  }
  if info.at("module_link", default: none) != none {
    return info.module_link
  }

  none
}

#let module-heading-title(info) = {
  if info.prefix.len() > 0 {
    "Module: " + info.prefix
  } else {
    "Package Exports"
  }
}

#let module-title(info) = {
  if info.prefix.len() > 0 {
    "Module: " + info.prefix
  } else {
    "Package Exports"
  }
}

#let module-nav-title(info) = {
  if info.prefix.len() == 0 {
    return "Package Exports"
  }

  let path = info.at("path", default: none)
  if path == none {
    return module-title(info)
  }

  path = str(path)
  if path.ends-with(".typ") {
    path.slice(0, -4)
  } else {
    path
  }
}

#let module-anchor(info) = {
  "#" + str(heading-label(module-heading-title(info)))
}

#let module-output-path(index, info) = {
  if index == 0 {
    return "index.html"
  }

  let path = info.at("path", default: none)
  if path == none {
    return "module-" + str(index) + ".html"
  }

  path = str(path)
  if path.ends-with(".typ") {
    path.slice(0, -4) + ".html"
  } else {
    path + ".html"
  }
}

#let module-path-stem(index, info) = {
  if index == 0 {
    return none
  }

  let path = info.at("path", default: none)
  if path == none {
    return "module-" + str(index)
  }

  path = str(path)
  if path.ends-with(".typ") {
    path.slice(0, -4)
  } else {
    path
  }
}

#let symbol-file-stem(raw) = {
  let stem = str(raw).replace(regex("[^A-Za-z0-9_-]+"), "-").replace(regex("-+"), "-")
  if stem == "" or stem == "-" {
    "symbol"
  } else {
    stem
  }
}

#let module-symbol-output-path(index, info, section, symbol) = {
  let file = symbol-file-stem(symbol) + ".html"
  if index == 0 {
    return section + "/" + file
  }

  module-path-stem(index, info) + "/" + section + "/" + file
}

#let module-source-output-path(info) = {
  let path = info.at("path", default: none)
  if path == none {
    return none
  }

  str(path) + ".html"
}

#let module-nav-dest(symbol-ctx, index, info) = {
  if symbol-ctx.at("bundle", default: false) {
    let output = module-output-path(index, info)
    if symbol-ctx.at("path", default: none) == output {
      return module-anchor(info)
    }

    return relative-path(symbol-ctx.at("path", default: none), output)
  }

  module-anchor(info)
}

#let module-source-dest(symbol-ctx, info) = {
  if not symbol-ctx.at("bundle", default: false) {
    return none
  }

  let output = module-source-output-path(info)
  if output == none {
    return none
  }

  relative-path(symbol-ctx.at("path", default: none), output)
}

#let source-line-id(line) = {
  "L" + str(line)
}

#let source-line-fragment(line) = {
  "#" + source-line-id(line)
}

#let module-source-line-dest(symbol-ctx, line) = {
  let module-info = symbol-ctx.at("module-info", default: none)
  if module-info == none {
    return none
  }

  let dest = module-source-dest(symbol-ctx, module-info)
  if dest == none {
    return none
  }

  dest + source-line-fragment(line)
}

#let source-query-line-dest(symbol-ctx, source, line) = {
  if not symbol-ctx.at("bundle", default: false) {
    return none
  }

  let file = symbol-ctx.files.at(source.file, default: none)
  if file != none and file.at("path", default: none) != none {
    return relative-path(symbol-ctx.at("path", default: none), str(file.path) + ".html") + source-line-fragment(line)
  }

  module-source-line-dest(symbol-ctx, line)
}

#let module-section-list = (
  ("constants", "Constants"),
  ("functions", "Functions"),
)

#let section-title(section) = {
  for (id, title) in module-section-list {
    if id == section {
      return title
    }
  }

  section
}

#let symbol-section(info) = {
  if info.kind == "function" and not info.name.starts-with("_") {
    return "functions"
  }
  if (info.kind == "variable" or info.kind == "constant") and not info.name.starts-with("_") {
    return "constants"
  }

  none
}

#let module-section-items(m, section) = {
  let items = ()
  for child in m.children {
    if symbol-section(child) == section {
      items.push(child)
    }
  }

  items
}

#let module-has-section(m, section) = {
  module-section-items(m, section).len() > 0
}

#let symbol-heading-anchor(in-module, info) = {
  "#" + str(heading-label(info.kind + ": " + info.name + " in " + in-module))
}

#let sidebar-symbol-label(info) = {
  info.name
}

#let symbol-anchor(symbol-ctx, info) = {
  symbol-heading-anchor(symbol-ctx.in-module, info)
}

#let symbol-section-dest(symbol-ctx, info) = {
  let dest = symbol-dest(symbol-ctx, info)
  if dest != none {
    return dest
  }

  let section = symbol-section(info)
  if section == none {
    return none
  }

  let module-index = symbol-ctx.at("module-index", default: none)
  let module-info = symbol-ctx.at("module-info", default: none)
  if module-index == none or module-info == none {
    return symbol-anchor(symbol-ctx, info)
  }

  module-symbol-output-path(module-index, module-info, section, info.name)
}

#let analyze-package(p) = {
  (
    packages: p.packages,
    files: p.files,
  )
}

#let analyze-module(m) = {
  let modules = ()
  let functions = ()
  let constants = ()
  let unknowns = ()

  for child in m.children {
    if child.kind == "module" {
      modules.push(child)
    } else if child.kind == "function" {
      if not child.name.starts-with("_") {
        functions.push(child)
      }
    } else if child.kind == "variable" or child.kind == "constant" {
      if not child.name.starts-with("_") {
        constants.push(child)
      }
    } else {
      unknowns.push(child)
    }
  }

  (
    modules: modules.sorted(key: it => it.id),
    functions: functions,
    constants: constants,
    unknowns: unknowns,
  )
}
