// Test that recursive functions work correctly
#let factorial(n) = {
  if n <= 1 {
    1
  } else {
    n * factorial(n - 1)
  }
}

#factorial(5)
