/// path: base.typ
#let scale-values(values, factor) = values.map(value => value * factor)
#let seeded = scale-values(range(2), 2)

-----
#import "base.typ": scale-values

#let use-scale(values) = scale-values(values, 2)
#let result = use-scale
