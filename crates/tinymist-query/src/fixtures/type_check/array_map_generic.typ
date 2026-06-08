/// - s (array(int)): The array.
#let f(s) = s.map(str)

#let a = (1, 2)
#let b = a.map(str)

#let c = (1, "s")
#let d = c.map(str)

#let id(x) = x
#let e = c.map(id)
