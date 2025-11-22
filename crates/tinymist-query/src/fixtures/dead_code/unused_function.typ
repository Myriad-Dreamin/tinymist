// Test unused function detection
#let unused_func() = {
  "never called"
}

#let used_func() = {
  "called below"
}

#used_func()
