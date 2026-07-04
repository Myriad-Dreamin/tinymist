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

#let x3(cond) = {
  if cond {
    return 1
  }
  2
}

#let x4(cond) = {
  if cond {
    return 1
  } else {
    return 2
  }
  3
}

#let x5(cond) = {
  for i in range(2) {
    if cond {
      return 1
    }
    2
  }
}

#let x6(cond) = context {
  if cond {
    return 1
  }
  2
}

#let x7(p) = {
  if type(p) == array {
    if p.len() >= 2 {
      return if p.len() == 2 {
        (p.at(0), p.at(1), 0)
      } else {
        p
      }
    }
    let (_, pt) = p
    return pt
  }
  panic("x")
}

#let x8(cond) = {
  if cond {
    return 1
  }
  panic("x")
}
