#import "mod.typ": *
#import "/typ/packages/tinymist-index/lib.typ": file-by-uri, index-definition, index-hover

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
    return source-query-line-dest(symbol-ctx, source, source.position.line + 1)
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
  source-query-line-dest(symbol-ctx, source, definition.targetSelectionRange.start.line + 1)
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
