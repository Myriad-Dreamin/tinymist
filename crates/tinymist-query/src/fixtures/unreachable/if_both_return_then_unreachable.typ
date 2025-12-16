// path: unreachable/if_both_return_then_unreachable.typ
#let f() = {
  if true {
    return 1
  } else {
    return 2
  }
  3
}
