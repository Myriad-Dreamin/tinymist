// path: unreachable/if_one_return_still_reachable.typ
#let f(a) = {
  if a {
    return 1
  } else {
    2
  }
  3
}
