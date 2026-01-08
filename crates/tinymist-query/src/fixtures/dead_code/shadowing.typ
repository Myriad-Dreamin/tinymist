// Test variable shadowing
#let x = 1  // This outer x is unused

#{
  let x = 2  // This shadows outer x and is used
  x + 1
}
