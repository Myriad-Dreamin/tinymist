#let f(x, y: none) = x + y
#let g = f.with(1)
#(/* position after */ g(y: 1))