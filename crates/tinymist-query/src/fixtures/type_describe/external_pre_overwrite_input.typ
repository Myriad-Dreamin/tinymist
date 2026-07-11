/// path: base.typ
#let normalize(value) = {
  assert(type(value) == float)
  calc.max(value, 0.1)
  value = 3
  value
}

-----
#import "base.typ": normalize

#(/* position after */ normalize)
