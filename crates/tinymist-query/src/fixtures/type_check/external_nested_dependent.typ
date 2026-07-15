/// path: base.typ
#let make-pair(left) = right => (left, right)

-----
#import "base.typ": make-pair

#let pair-builder = make-pair(1)
#let pair = pair-builder("right")
#let first = pair.at(0)
#let second = pair.at(1)
