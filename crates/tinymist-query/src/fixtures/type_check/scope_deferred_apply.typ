#let f() = {
  let id(x) = x
  return id(1)
}

#let h() = {
  let choose(x, y) = if true { x } else { y }
  return choose(1, "x")
}

#let k() = {
  let fill(d) = {
    if "x" not in d {
      d.insert("x", 1)
    }
    d
  }
  let entry = (:)
  return fill(entry)
}

#let m() = {
  let complete(key, definitions) = {
    if "long" not in definitions {
      panic("x")
    }
    if "short" not in definitions {
      definitions.insert("short", key)
    }
    definitions
  }
  let entry = (:)
  return complete("a", entry)
}

#let n() = {
  let complete(key, definitions) = {
    if "long" not in definitions {
      panic("x")
    }
    if "short" not in definitions {
      definitions.insert("short", key)
    }
    if "short-pl" not in definitions {
      definitions.insert("short-pl", [#definitions.at("short")\s])
    }
    definitions
  }
  let entry = (:)
  return complete("a", entry)
}

#let top-complete(key, definitions) = {
  if "long" not in definitions {
    panic("x")
  }
  if "short" not in definitions {
    definitions.insert("short", key)
  }
  if "short-pl" not in definitions {
    definitions.insert("short-pl", [#definitions.at("short")\s])
  }
  if "long-pl" not in definitions {
    definitions.insert("long-pl", [#definitions.at("long")\s])
  }
  definitions
}

#let p(definitions) = {
  let entry = (:)
  return top-complete("a", entry)
}

#let q(definitions) = {
  let entry = (:)
  top-complete("a", entry)
}

#let r(definitions) = {
  let entry = (:)
  if type(definitions) == str {
    entry.insert("long", definitions)
  } else if type(definitions) == array {
    let n_defs = definitions.len()
    if n_defs == 0 {
      panic("x")
    } else if n_defs > 2 {
      panic("x")
    }
    for (key, def) in ("long", "long-pl").zip(definitions) {
      entry.insert(key, def)
    }
  } else if type(definitions) == dictionary {
    if "long" not in definitions {
      panic("x")
    }
    for (key, def) in definitions {
      if key in ("short", "short-pl", "long", "long-pl") {
        entry.insert(key, def)
      } else {
        panic("x")
      }
    }
  } else {
    panic("x")
  }
  top-complete("a", entry)
}
