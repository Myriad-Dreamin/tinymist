/// path: base.typ
#let fixed = (1, 2)
-----
#import "base.typ": fixed

#let copied = fixed
#let first = fixed.at(0)
