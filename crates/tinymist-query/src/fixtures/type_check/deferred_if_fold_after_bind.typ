#let make-path(mode: "OPEN") = {
  let close = mode != "OPEN"
  (close: close)
}

#let default-path = make-path()
#let pie-path = make-path(mode: "PIE")
