#let add(a, b) = a + b
#let sub(a, b) = a - b
#let mul(a, b) = a * b

#let test-add() = {
  assert(add(1, 2) == 3)
}

#let test-mul() = {
  assert(mul(2, 3) == 6)
}
