#let f(x, y: none) = x + y
#let g = f.with(y: 1)
#(/* position after */ g(1))