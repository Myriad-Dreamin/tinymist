#import "@preview/cmarker:0.1.6": render
#import "/typ/packages/tinymist-index/lib.typ": create_index, file-by-uri, index-definition, index-hover, index-public-symbols, query
// re-export page template
#import "/typ/packages/package-docs/template.typ": project

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

#let package-sidebar(info, symbol-ctx, active-module-index: none, active-section: none, active-symbol-index: none) = {
  let package-link = if symbol-ctx.at("bundle", default: false) {
    let output = module-output-path(0, info.modules.at(0).at(2))
    if symbol-ctx.at("path", default: none) == output {
      module-anchor(info.modules.at(0).at(2))
    } else {
      relative-path(symbol-ctx.at("path", default: none), output)
    }
  } else {
    module-anchor(info.modules.at(0).at(2))
  }

  html.elem("aside", attrs: (class: "package-sidebar"), [
    #html.elem("nav", attrs: (class: "package-sidebar-nav", "aria-label": "Package modules"), [
      #html.elem("a", attrs: (class: "package-sidebar-title", href: package-link), [
        #html.elem("span", attrs: (class: "package-sidebar-package"), "@" + info.meta.namespace + "/" + info.meta.name)
        #html.elem("span", attrs: (class: "package-sidebar-version"), info.meta.version)
      ])
      #html.elem("div", attrs: (class: "package-sidebar-section"), "Modules")
      #html.elem("ul", attrs: (class: "package-sidebar-list"), [
        #let index = 0
        #for module-entry in info.modules {
          let module-info = module-entry.at(2)
          let module-def = module-entry.at(1)
          let class = "package-sidebar-link"
          if active-module-index == index {
            class = class + " is-active"
          }

          html.elem("li", attrs: (class: "package-sidebar-item"), [
            #html.elem(
              "a",
              attrs: (class: class, href: module-nav-dest(symbol-ctx, index, module-info)),
              module-nav-title(module-info),
            )
            #if active-module-index == index {
              html.elem("ul", attrs: (class: "package-sidebar-sublist"), [
                #for (section, title) in module-section-list {
                  let items = module-section-items(module-def, section)
                  if items.len() > 0 {
                    let section-class = "package-sidebar-sublink"
                    if active-section == section {
                      section-class = section-class + " is-active"
                    }
                    let section-symbol-ctx = (
                      in-module: module-info.prefix,
                      module-index: index,
                      module-info: module-info,
                      ..symbol-ctx,
                    )
                    html.elem("li", attrs: (class: "package-sidebar-subitem"), [
                      #html.elem("span", attrs: (class: section-class), title)
                      #html.elem("ul", attrs: (class: "package-sidebar-symbol-list"), [
                        #let item-index = 0
                        #for child in items {
                          let symbol-class = "package-sidebar-symbol-link"
                          if active-section == section and active-symbol-index == item-index {
                            symbol-class = symbol-class + " is-active"
                          }
                          html.elem("li", attrs: (class: "package-sidebar-symbol-item"), [
                            #html.elem(
                              "a",
                              attrs: (
                                class: symbol-class,
                                href: symbol-section-dest(section-symbol-ctx, child),
                              ),
                              sidebar-symbol-label(child),
                            )
                          ])
                          item-index += 1
                        }
                      ])
                    ])
                  }
                }
              ])
            }
          ])
          index += 1
        }
      ])
    ])
  ])
}

#let package-on-this-page() = {
  html.elem("aside", attrs: (class: "package-on-this-page", "data-ready": "false"), [
    #html.elem("nav", attrs: (class: "package-on-this-page-nav", "aria-label": "On this page"), [
      #html.elem("div", attrs: (class: "package-on-this-page-title"), "On this page")
      #html.elem("div", attrs: (class: "package-on-this-page-body"), [
        #html.elem("span", attrs: (class: "package-on-this-page-indicator", "aria-hidden": "true"))
        #html.elem("ul", attrs: (class: "package-on-this-page-list"))
      ])
    ])
    #html.elem("script", attrs: (type: "module"), read("/typ/packages/package-docs/on-this-page.js"))
  ])
}

#let package-layout(info, symbol-ctx, active-module-index: none, active-section: none, active-symbol-index: none, body) = {
  html.elem("div", attrs: (class: "package-doc-layout"), [
    #package-sidebar(
      info,
      symbol-ctx,
      active-module-index: active-module-index,
      active-section: active-section,
      active-symbol-index: active-symbol-index,
    )
    #html.elem("main", attrs: (class: "package-doc-main"), body)
    #package-on-this-page()
  ])
}

#let hover-oneliner(symbol-ctx, info) = {
  let docs = index-hover(symbol-ctx, info)
  if docs != none {
    let in-code = false
    for line in str(docs).split("\n") {
      let text = line.trim()
      if text.starts-with("```") {
        in-code = not in-code
      } else if not in-code and text != "" and text != "---" and not text.starts-with("#") {
        return text
      }
    }
  }

  info.at("oneliner", default: none)
}

#let symbol-defined-path(symbol-ctx, info) = {
  let source = info.at("source", default: none)
  if source != none {
    let file = symbol-ctx.files.at(source.file, default: none)
    if file != none and file.at("path", default: none) != none {
      return str(file.path)
    }
  }

  let bundle-link = info.at("bundle_link", default: none)
  if bundle-link != none {
    let path = str(bundle-link)
    if path.ends-with(".html") {
      path = path.slice(0, -5) + ".typ"
    }
    if path.starts-with("symbols/") {
      return none
    }
    return path
  }

  let module-info = symbol-ctx.at("module-info", default: none)
  if module-info != none and module-info.at("path", default: none) != none {
    return str(module-info.path)
  }

  none
}

#let symbol-reference-summary(symbol-ctx, info) = {
  let summary = hover-oneliner(symbol-ctx, info)
  if summary != none {
    return summary
  }

  let path = symbol-defined-path(symbol-ctx, info)
  if path != none {
    return [Defined in #code(path).]
  }

  none
}

#let symbol-reference(symbol-ctx, info) = {
  let dest = symbol-section-dest(symbol-ctx, info)
  let summary = symbol-reference-summary(symbol-ctx, info)
  let title = if info.kind == "function" {
    info.name + "()"
  } else {
    info.name
  }

  html.elem("li", attrs: (class: "symbol-reference doc-symbol-" + info.kind), [
    #html.elem("a", attrs: (class: "symbol-reference-link", href: dest), code(title))
    #if summary != none {
      html.elem("span", attrs: (class: "symbol-reference-summary"), summary)
    }
  ])
}

#let symbol-reference-list(symbol-ctx, items) = {
  html.elem("ul", attrs: (class: "symbol-reference-list"), [
    #for child in items {
      symbol-reference(symbol-ctx, child)
    }
  ])
}

#let function-hover-join(lines) = {
  if lines.len() == 0 {
    ""
  } else {
    lines.join("\n").trim()
  }
}

#let function-hover-param-name(raw) = {
  str(raw).replace(" (positional)", "").replace(" (named)", "").replace(" (rest)", "").trim()
}

#let function-hover-param(body, name) = {
  (
    name: function-hover-param-name(name),
    body: function-hover-join(body),
  )
}

#let function-hover-signature-param-name(line) = {
  let text = line.trim()
  if text == "" or text.starts-with("let ") or text.starts-with(")") {
    return none
  }
  if text.ends-with(",") {
    text = text.slice(0, -1)
  }
  if text.starts-with("..") {
    text = text.slice(2)
  }

  let parts = text.split(":")
  if parts.len() == 0 {
    return none
  }

  parts.at(0).trim()
}

#let function-hover-signature-param-line(signature, name) = {
  for line in signature.split("\n") {
    let found = function-hover-signature-param-name(line)
    if found == name {
      let text = line.trim()
      if text.ends-with(",") {
        text = text.slice(0, -1)
      }
      return text
    }
  }

  none
}

#let function-hover-signature-param-line-offset(signature, name) = {
  let found-offset = none
  let param-count = 0
  let index = 0

  for line in signature.split("\n") {
    let found = function-hover-signature-param-name(line)
    if found != none {
      param-count += 1
      if found == name {
        found-offset = index
      }
    }
    index += 1
  }

  if found-offset == none {
    return 0
  }

  if param-count == 1 {
    return 0
  }

  found-offset
}

#let function-hover-signature-param-meta(signature, name) = {
  let line = function-hover-signature-param-line(signature, name)
  if line == none {
    return (ty: none, default: none, required: true)
  }

  let parts = line.split(":")
  if parts.len() < 2 {
    return (ty: none, default: none, required: true)
  }

  let right = parts.slice(1).join(":").trim()
  let default = none
  let ty = right
  let default-parts = right.split(" = ")
  if default-parts.len() > 1 {
    ty = function-hover-join(default-parts.slice(0, -1))
    default = default-parts.at(default-parts.len() - 1).trim()
  }

  (ty: ty, default: default, required: default == none)
}

#let function-param-body-data(body) = {
  let lines = str(body).split("\n")
  let ty = none
  let docs = ()
  let i = 0

  if lines.len() > 0 and lines.at(0).starts-with("```") {
    i = 1
    let type-lines = ()
    while i < lines.len() and not lines.at(i).starts-with("```") {
      type-lines.push(lines.at(i))
      i += 1
    }
    if i < lines.len() {
      i += 1
    }

    let type-text = function-hover-join(type-lines)
    if type-text.starts-with("type:") {
      type-text = type-text.slice(5).trim()
    }
    if type-text != "" {
      ty = type-text
    }
  }

  while i < lines.len() and lines.at(i).trim() == "" {
    i += 1
  }
  while i < lines.len() {
    docs.push(lines.at(i))
    i += 1
  }

  (ty: ty, docs: function-hover-join(docs))
}

#let function-hover-data(markdown) = {
  let lines = str(markdown).split("\n")
  let signature = ()
  let docs = ()
  let positional = ()
  let named = ()
  let rest = ()

  let i = 0
  if lines.len() > 0 and lines.at(0).starts-with("```") {
    i = 1
    while i < lines.len() and not lines.at(i).starts-with("```") {
      signature.push(lines.at(i))
      i += 1
    }
    if i < lines.len() {
      i += 1
    }
  }

  while i < lines.len() and (lines.at(i).trim() == "" or lines.at(i).trim() == "---") {
    i += 1
  }

  let section = none
  let param-name = none
  let param-lines = ()

  while i < lines.len() {
    let line = lines.at(i)
    let text = line.trim()

    if text == "# Positional Parameters" or text == "# Named Parameters" or text == "# Rest Parameters" {
      if param-name != none {
        let item = function-hover-param(param-lines, param-name)
        if section == "positional" {
          positional.push(item)
        } else if section == "named" {
          named.push(item)
        } else if section == "rest" {
          rest.push(item)
        }
      }

      section = if text == "# Positional Parameters" {
        "positional"
      } else if text == "# Named Parameters" {
        "named"
      } else {
        "rest"
      }
      param-name = none
      param-lines = ()
    } else if section != none and text.starts-with("## ") {
      if param-name != none {
        let item = function-hover-param(param-lines, param-name)
        if section == "positional" {
          positional.push(item)
        } else if section == "named" {
          named.push(item)
        } else if section == "rest" {
          rest.push(item)
        }
      }

      param-name = text.slice(3)
      param-lines = ()
    } else if section == none {
      docs.push(line)
    } else if param-name != none {
      param-lines.push(line)
    }

    i += 1
  }

  if param-name != none {
    let item = function-hover-param(param-lines, param-name)
    if section == "positional" {
      positional.push(item)
    } else if section == "named" {
      named.push(item)
    } else if section == "rest" {
      rest.push(item)
    }
  }

  (
    signature: function-hover-join(signature),
    docs: function-hover-join(docs),
    positional: positional,
    named: named,
    rest: rest,
  )
}

#let function-param-id(info, group, param) = {
  str(heading-label("parameter " + group + " " + param.name + " in " + info.name))
}

#let function-heading-id(info) = {
  str(heading-label("Function " + info.name))
}

#let function-section-id(info, group) = {
  str(heading-label(group + " parameters in " + info.name))
}

#let function-toc-heading(level, text, id, toc-depth: 0) = {
  html.elem(
    "h" + str(level),
    attrs: (
      id: id,
      class: "package-function-heading",
      "data-toc-label": text,
      "data-toc-depth": str(toc-depth),
    ),
    text,
  )
}

#let function-find-param(data, name) = {
  for param in data.positional {
    if param.name == name {
      return (group: "positional", ..param)
    }
  }
  for param in data.named {
    if param.name == name {
      return (group: "named", ..param)
    }
  }
  for param in data.rest {
    if param.name == name {
      return (group: "rest", ..param)
    }
  }

  none
}

#let function-param-in(items, name) = {
  for item in items {
    if item.name == name {
      return true
    }
  }

  false
}

#let function-ordered-params(data) = {
  let items = ()
  for line in data.signature.split("\n") {
    let name = function-hover-signature-param-name(line)
    if name != none and not function-param-in(items, name) {
      let param = function-find-param(data, name)
      if param != none {
        items.push(param)
      }
    }
  }

  for param in data.positional {
    if not function-param-in(items, param.name) {
      items.push((group: "positional", ..param))
    }
  }
  for param in data.named {
    if not function-param-in(items, param.name) {
      items.push((group: "named", ..param))
    }
  }
  for param in data.rest {
    if not function-param-in(items, param.name) {
      items.push((group: "rest", ..param))
    }
  }

  items
}

#let function-param-label(data, param) = {
  if param.group == "named" {
    let meta = function-hover-signature-param-meta(data.signature, param.name)
    if meta.required {
      param.name
    } else {
      param.name + "?"
    }
  } else if param.group == "rest" {
    ".." + param.name
  } else {
    param.name
  }
}

#let function-param-mods(group, meta) = {
  let mods = ()
  if meta.required and group != "rest" {
    mods.push("required")
  }

  mods
}

#let function-param-type-parts(ty) = {
  let ty = str(ty)
  if ty.contains("=>") {
    return (ty,)
  }

  let parts = ()
  for chunk in ty.split("|") {
    for part in chunk.split(",") {
      let part = part.trim()
      if part != "" {
        parts.push(part)
      }
    }
  }

  parts
}

#let function-param-pill-class(part) = {
  let part = str(part).trim()
  if part == "content" {
    "function-param-pill-content"
  } else if part == "str" or part == "string" {
    "function-param-pill-string"
  } else if part == "auto" or part == "none" or part == "bool" or part == "boolean" {
    "function-param-pill-keyword"
  } else if part == "int" or part == "float" or part == "number" or part == "length" or part == "ratio" or part == "angle" {
    "function-param-pill-number"
  } else if part == "array" or part == "dictionary" or part == "arguments" {
    "function-param-pill-collection"
  } else if part == "function" or part.contains("=>") {
    "function-param-pill-function"
  } else {
    "function-param-pill-object"
  }
}

#let function-inline-code-text(value) = {
  html.elem("code", str(value))
}

#let function-inline-typc(value) = {
  let value = str(value)
  if value == "(:)" {
    function-inline-code-text(value)
  } else {
    raw(value, lang: "typc")
  }
}

#let function-param-type-list(ty) = {
  let parts = function-param-type-parts(ty)
  if parts.len() == 0 {
    parts.push(str(ty))
  }

  html.elem("span", attrs: (class: "function-param-type-list"), [
    #let first = true
    #for part in parts {
      if not first {
        html.elem("small", attrs: (class: "function-param-type-or"), "or")
      }
      first = false
      html.elem(
        "code",
        attrs: (class: "function-param-pill " + function-param-pill-class(part)),
        part,
      )
    }
  ])
}

#let function-param-source-dest(symbol-ctx, info, data, param) = {
  let source = info.at("source", default: none)
  if source != none {
    let line = source.position.line + 1 + function-hover-signature-param-line-offset(data.signature, param.name)
    return source-query-line-dest(symbol-ctx, source, line)
  }

  let definition = index-definition(symbol-ctx, info)
  if definition == none {
    return none
  }

  let file = file-by-uri(symbol-ctx.files, str(definition.targetUri))
  if file == none {
    return none
  }

  let source = (
    file: file.index,
    position: definition.targetSelectionRange.start,
  )
  let line = definition.targetSelectionRange.start.line + 1 + function-hover-signature-param-line-offset(data.signature, param.name)
  source-query-line-dest(symbol-ctx, source, line)
}

#let function-param-heading(symbol-ctx, info, data, group, param, meta, ty) = {
  let source-dest = function-param-source-dest(symbol-ctx, info, data, param)

  html.elem(
    "h4",
    attrs: (
      id: function-param-id(info, group, param),
      class: "package-function-heading function-param-heading",
      "data-toc-label": param.name,
      "data-toc-depth": "1",
    ),
    [
      #html.elem("code", attrs: (class: "function-param-name"), param.name)
      #html.elem(
        "div",
        attrs: (class: "function-param-additional-info"),
        [
          #function-param-type-list(ty)
          #for mod in function-param-mods(group, meta) {
            html.elem("small", attrs: (class: "function-param-mod"), mod)
          }
          #if source-dest != none {
            html.elem("a", attrs: (class: "function-param-source-link", href: source-dest), "Source")
          }
        ],
      )
      #if meta.default != none and group != "rest" {
        html.elem("span", attrs: (class: "function-param-default"), [
          #html.elem("span", attrs: (class: "function-param-default-label"), "Default")
          #html.elem("span", attrs: (class: "function-param-default-code"), function-inline-typc(meta.default))
        ])
      }
    ],
  )
}

#let function-signature-nav(symbol-ctx, info, data) = {
  let params = function-ordered-params(data)

  html.elem("div", attrs: (class: "function-signature-nav"), [
    #html.elem("code", [
      #html.elem("span", attrs: (class: "function-signature-name"), info.name)
      #html.elem("span", "(")
      #let first = true
      #for param in params {
        if not first {
          html.elem("span", attrs: (class: "function-signature-punctuation"), ", ")
        }
        first = false
        let href = "#" + function-param-id(info, param.group, param)
        html.elem("a", attrs: (class: "function-signature-param", href: href), function-param-label(data, param))
      }
      #html.elem("span", ")")
    ])
  ])
}

#let function-signature-block(signature) = {
  if signature != "" {
    html.elem("div", attrs: (class: "function-signature-block"), [
      #raw(signature, lang: "typc", block: true)
    ])
  }
}

#let function-param-doc(symbol-ctx, info, data, group, param) = {
  let body = function-param-body-data(param.body)
  let meta = function-hover-signature-param-meta(data.signature, param.name)
  let ty = if body.ty != none {
    body.ty
  } else if meta.ty != none and meta.ty != "" {
    meta.ty
  } else {
    "unknown"
  }

  html.elem("section", attrs: (class: "function-param-doc function-param-" + group), [
    #function-param-heading(symbol-ctx, info, data, group, param, meta, ty)
    #if body.docs != "" {
      html.elem("div", attrs: (class: "function-param-body"), [
        #markdown-docs(body.docs)
      ])
    }
  ])
}

#let function-param-section(symbol-ctx, info, data, group, title, params) = {
  if params.len() > 0 {
    function-toc-heading(3, title, function-section-id(info, group), toc-depth: 0)
    for param in params {
      function-param-doc(symbol-ctx, info, data, group, param)
    }
  }
}

#let function-symbol-page(symbol-ctx, info) = {
  let plain-docs = index-hover(symbol-ctx, info)
  let data = if plain-docs == none {
    (signature: "", docs: "", positional: (), named: (), rest: ())
  } else {
    function-hover-data(plain-docs)
  }

  html.elem("article", attrs: (class: "package-function-doc"), [
    #function-toc-heading(2, "Function " + info.name, function-heading-id(info), toc-depth: 0)
    #html.elem("div", attrs: (class: "detail-header doc-symbol-function function-signature-header"), [
      #function-signature-nav(symbol-ctx, info, data)
    ])
    #if data.docs != "" {
      html.elem("div", attrs: (class: "function-docs"), [
        #markdown-docs(data.docs)
      ])
    }
    #function-signature-block(data.signature)
    #function-param-section(symbol-ctx, info, data, "positional", "Positional Parameters", data.positional)
    #function-param-section(symbol-ctx, info, data, "named", "Named Parameters", data.named)
    #function-param-section(symbol-ctx, info, data, "rest", "Rest Parameters", data.rest)
  ])
}

#let symbol-doc(symbol-ctx, info) = {
  // let ident = if !primary.is_empty() {
  //     eco_format!("symbol-{}-{primary}.{}", child.kind, child.name)
  // } else {
  //     eco_format!("symbol-{}-{}", child.kind, child.name)
  // };

  let symlink(body) = if symbol-dest(symbol-ctx, info) != none {
    html.elem("a", attrs: (href: symbol-dest(symbol-ctx, info), class: "symbol-link"), body)
  } else {
    body
  }

  if info.is_external {
    let definition = index-definition(symbol-ctx, info)
    let file = if definition != none {
      file-by-uri(symbol-ctx.files, str(definition.targetUri))
    } else {
      none
    }
    if file == none and info.at("source", default: none) != none {
      file = symbol-ctx.files.at(info.source.file, default: none)
    }
    let package = if file != none {
      symbol-ctx.packages.at(file.package, default: none)
    } else {
      none
    }
    let file-title = if file != none {
      code(file.path)
    } else if definition != none {
      code(str(definition.targetUri))
    } else {
      code(info.name)
    }

    let title = if info.kind == "module" {
      let title = if file != none and package != none and file.package > 0 {
        span(attrs: (title: display-package-spec(package)), "external")
        code(" ")
        code(file.path)
      } else {
        file-title
      }

      symlink(code(title))
    } else {
      // keyword("extern")
      // code(" ")
      symlink(code(info.name))
      if info.kind == "function" {
        code("()")
      } else {
        code(": ")
        builtin-ty[any]
      }
    }

    html.elem("div", attrs: (class: "detail-header doc-symbol-" + info.kind), [=== #title])

    let plain-docs = index-hover(symbol-ctx, info)
    if plain-docs != none {
      markdown-docs(plain-docs)
    }

    return
  }

  labelled-heading(
    3,
    info.kind + ": " + info.name + " in " + symbol-ctx.in-module,
    dest: symbol-dest(symbol-ctx, info),
  )

  // if info.symbol_link != none {
  //   // let _ = writeln!(out, "#link({})[Symbol Docs]\n", TypstLink(lnk));
  //   par(symlink("Symbol Docs"))
  // }

  let plain-docs = index-hover(symbol-ctx, info)
  if plain-docs != none {
    markdown-docs(plain-docs)
  }
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
  html.elem("style", read("/typ/packages/package-docs/global.css"))
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

#let package-doc(info, scip: none) = {
  let info = json(info)
  let index = if scip == none {
    none
  } else {
    create_index(scip)
  }
  let title = package-title(info)

  package-setup(title)

  let symbol-ctx = symbol-context(info, index)

  package-layout(info, symbol-ctx)[
    #package-header(info, title)
    #for (name, m, info) in info.modules {
      module-doc(info: info, name: name, symbol-ctx, m)
    }
  ]
}
