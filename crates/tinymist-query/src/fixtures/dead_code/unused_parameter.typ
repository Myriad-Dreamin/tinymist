// Test unused function parameter detection
#let func(used_param, unused_param, _intentionally_unused) = {
  used_param + 1
}

#func(10, 20, 30)
