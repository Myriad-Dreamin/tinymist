#let check-free-var(v) = {
  if v == "i" {
    panic("reserved variable")
  }
}

#let public(expr, var) = {
  check-free-var(var)
  expr
}

#let use-public = public(1, "x")
