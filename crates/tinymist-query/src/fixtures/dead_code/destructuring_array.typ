// Test complex destructuring patterns
#let data = (
  (1, 2),
  (3, 4, 5),
)

#let ((used_a, unused_x), (c1, c2, c3)) = data

#used_a
#c1
#c3
// c2 and unused_x should be warned
