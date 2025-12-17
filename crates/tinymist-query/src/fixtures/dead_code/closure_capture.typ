// Test closures capturing variables
#let captured = 42
#let unused_not_captured = 100

#let closure = () => {
  captured + 1
}

#closure()
