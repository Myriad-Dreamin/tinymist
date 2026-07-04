#let loop-none() = {
  for i in range(4) {
    let x = i
  }
}

#let loop-tuple(size: 50) = {
  for i in range(4) {
    ((type: "move", steps: size), (type: "turn", degrees: 90))
  }
}

#let loop-command() = {
  for i in range(2) {
    (ctx => ctx,)
  }
}

#let loop-continue-command() = {
  for i in range(2) {
    if i == 0 { continue }
    (ctx => ctx,)
  }
}

#let loop-conditional-break() = {
  for i in range(2) {
    if i == 1 { break }
    (ctx => ctx,)
  }
}

#let loop-break() = {
  for i in range(2) {
    break
  }
  3
}

#let while-false() = {
  while false {
    [x]
  }
  3
}
