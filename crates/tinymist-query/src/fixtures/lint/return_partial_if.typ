#let get-op(offset, op) = {
  if op == none {
    op = if offset < 0 { "<=" } else { ">=" }
  }

  return op
}
