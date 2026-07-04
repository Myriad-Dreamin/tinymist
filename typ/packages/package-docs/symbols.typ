#import "mod.typ": *
#import "function.typ": function-symbol-page
#import "/typ/packages/tinymist-index/lib.typ": file-by-uri, index-definition, index-hover

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
