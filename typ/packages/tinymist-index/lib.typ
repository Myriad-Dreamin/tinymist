
#let index-plugin = plugin("tinymist_index.wasm")

/// Creates an index.
/// - db-data (bytes): The data of the (database) index.
#let create_index(db-data) = plugin.transition(index-plugin.create_index, db-data, bytes(""))

/// Queries the index.
/// - db (any): The database.
/// - kind (str): The kind of the query.
/// - request (any): The request for the query.
#let query(db, kind, request) = json(db.query_index(bytes(kind), bytes(json.encode(request))))

/// Finds a file entry by URI.
/// - files (array): The file entries from the index metadata.
/// - uri (str): The file URI to find.
#let file-by-uri(files, uri) = {
  let index = 0
  for file in files {
    if file.at("uri", default: none) == uri {
      return (index: index, ..file)
    }
    index += 1
  }
  none
}

/// Queries public symbols for a module.
/// - symbol-ctx (dictionary): The package-doc symbol context.
/// - module-info (dictionary): The module metadata.
#let index-public-symbols(symbol-ctx, module-info) = {
  let db = symbol-ctx.at("index", default: none)
  if db == none {
    return ()
  }

  let path = module-info.at("path", default: none)
  if path == none {
    return ()
  }

  let result = query(db, "public_symbols", str(path))
  if result == none {
    ()
  } else {
    result
  }
}

/// Queries source tokens for a module source page.
/// - symbol-ctx (dictionary): The package-doc symbol context.
/// - source-path (str): The module source path.
#let index-source-tokens(symbol-ctx, source-path) = {
  let db = symbol-ctx.at("index", default: none)
  if db == none or source-path == none {
    return ()
  }

  let result = query(db, "source_tokens", str(source-path))
  if result == none {
    ()
  } else {
    result
  }
}

#let index-symbol-kind-matches(index-kind, info-kind) = {
  if index-kind == info-kind {
    return true
  }

  let value-kinds = ("constant", "variable")
  if value-kinds.contains(index-kind) and value-kinds.contains(info-kind) {
    return true
  }

  false
}

/// Resolves a package-doc symbol info item to an index symbol.
/// - symbol-ctx (dictionary): The package-doc symbol context.
/// - info (dictionary): The package-doc symbol info.
#let index-symbol(symbol-ctx, info) = {
  let symbol = info.at("symbol", default: none)
  if symbol != none {
    return symbol
  }

  let public-symbols = symbol-ctx.at("public-symbols", default: none)
  if public-symbols == none {
    let module-info = symbol-ctx.at("module-info", default: none)
    if module-info == none {
      return none
    }
    public-symbols = index-public-symbols(symbol-ctx, module-info)
  }

  for item in public-symbols {
    if item.name == info.name and index-symbol-kind-matches(item.kind, info.kind) {
      return item.symbol
    }
  }

  none
}

/// Queries a symbol definition from the index.
/// - symbol-ctx (dictionary): The package-doc symbol context.
/// - info (dictionary): The package-doc symbol info.
#let index-definition(symbol-ctx, info) = {
  let db = symbol-ctx.at("index", default: none)
  if db == none {
    return none
  }

  let symbol = index-symbol(symbol-ctx, info)
  if symbol != none {
    let result = query(db, "textDocument/definition", symbol)
    if result != none and result.len() > 0 {
      return result.at(0)
    }
  }

  let source = info.at("source", default: none)
  if source == none {
    return none
  }

  let file = symbol-ctx.files.at(source.file, default: none)
  if file == none or file.at("uri", default: none) == none {
    return none
  }

  let result = query(db, "textDocument/definition", (
    textDocument: (uri: file.uri),
    position: source.position,
  ))
  if result == none or result.len() == 0 {
    return none
  }

  result.at(0)
}

#let hover-markdown(value) = {
  if value == none {
    return none
  }
  if type(value) == str {
    return value
  }
  if type(value) == dictionary {
    let contents = value.at("contents", default: none)
    if contents != none {
      return hover-markdown(contents)
    }

    let text = value.at("value", default: none)
    if text != none {
      let language = value.at("language", default: none)
      if language != none {
        return "```" + str(language) + "\n" + text + "\n```"
      }
      return text
    }

    return none
  }
  if type(value) == array {
    let parts = ()
    for item in value {
      let part = hover-markdown(item)
      if part != none and part.len() > 0 {
        parts.push(part)
      }
    }
    if parts.len() > 0 {
      return parts.join("\n\n---\n\n")
    }
  }

  none
}

/// Queries hover markdown from the index.
/// - symbol-ctx (dictionary): The package-doc symbol context.
/// - info (dictionary): The package-doc symbol info.
#let index-hover(symbol-ctx, info) = {
  let db = symbol-ctx.at("index", default: none)
  if db == none {
    return none
  }

  let symbol = index-symbol(symbol-ctx, info)
  if symbol != none {
    let result = query(db, "textDocument/hover", symbol)
    return hover-markdown(result)
  }

  let source = info.at("source", default: none)
  if source == none {
    return none
  }

  let file = symbol-ctx.files.at(source.file, default: none)
  if file == none or file.at("uri", default: none) == none {
    return none
  }

  let result = query(db, "textDocument/hover", (
    textDocument: (uri: file.uri),
    position: source.position,
  ))

  hover-markdown(result)
}
