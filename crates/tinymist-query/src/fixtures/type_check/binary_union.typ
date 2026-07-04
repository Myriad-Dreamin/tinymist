#let f(flag) = {
  let x = if flag { 1 } else { "s" }
  x + x
}

#let sub-any(a, b) = a - b

#let mul-any(a, b) = a * b

#let cross(o, a, b) = (a.at(0) - o.at(0)) * (b.at(1) - o.at(1)) - (a.at(1) - o.at(1)) * (b.at(0) - o.at(0))

#let call-sub(a, b) = sub-any(a, b)
