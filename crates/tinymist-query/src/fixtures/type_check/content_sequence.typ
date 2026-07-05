#let joined-context() = context {
  [alpha]
  [beta]
}

#let mixed-context(x) = context {
  x
  [suffix]
}

#let return-or-content(cond) = context {
  if cond {
    return [warn]
  }
  [ok]
}

#let repeated-content-loops(items) = {
  for item in items {
    [#item]
  }
  for item in items {
    [#item]
  }
}

#let conditional-content-loop(items, cond) = {
  if cond {
    for item in items {
      [#item]
    }
  } else {
    [fallback]
  }
}
