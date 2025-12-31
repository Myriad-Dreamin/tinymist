// Test functions passed as values
#let helper(f) = {
  f(42)
}

#let used_callback(x) = x * 2
#let unused_callback(x) = x + 1

#helper(used_callback)
