/// path: base.typ
#let normalize(value) = {
  if value == auto {
    value = it => it
  } else {
    assert(type(value) == function)
  }
  return none
}

-----
#import "base.typ": normalize

#(/* position after */ normalize)
