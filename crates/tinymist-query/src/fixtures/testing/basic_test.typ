#let test-add() = {
  assert(1 + 1 == 2)
}

#let test-subtract() = {
  assert(5 - 3 == 2)
}

#let panic-on-error() = {
  panic("expected error")
}

#let example-simple() = {
  include "example-hello.typ"
}
