// Test variables used in conditionals
#let condition_var = true
#let used_in_if = 10
#let used_in_else = 20
#let unused = 30

#if condition_var {
  used_in_if
} else {
  used_in_else
}
