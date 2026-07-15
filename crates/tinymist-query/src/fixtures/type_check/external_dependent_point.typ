/// path: base.typ
#let coordinate-resolve(ctx, p) = (none, p)

#let resolve-point(ctx, p) = {
  let (_, point) = coordinate-resolve(ctx, p)
  point
}
-----
#import "base.typ": *

#let point = resolve-point(none, (1, 2, 3))
#let first = point.at(0)
