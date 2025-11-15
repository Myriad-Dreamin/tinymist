// Test complex destructuring patterns
#let data = (
  x: (1, 2),
  y: (3, 4, 5),
)

#let (x: (used_a, unused_x), y: (c1, c2, c3)) = data

#used_a
#c1
#c3
// c2 and unused_x should be warned
