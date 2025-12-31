// Test nested scopes
#let outer = 1

#{
  let inner_unused = 2
  let inner_used = 3
  
  inner_used + outer
}
