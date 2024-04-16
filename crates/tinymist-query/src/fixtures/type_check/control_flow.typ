#let x0 = if true {
  1
}
#let x1 = if false {
  2
}
#let x2 = context if here().page() > 0 {
  1
} else {
  2
}