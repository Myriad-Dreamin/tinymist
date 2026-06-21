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
#let x3() = {
  let x = none
  if true {
    x = 1
  } else {
    x = "s"
  }
  x
}
#let x4(cond) = {
  let w = none
  while cond {
    w = 1
  }
  w
}
#let x5() = {
  let y = none
  for item in (1, "s") {
    y = item
  }
  y
}
