#let currifyS(n, func) = {
  assert(type(n) == int and n >= 1)
  arg => if n == 1 {
    func(arg)
  } else {
    currifyS(n - 1, func.with(arg))
  }
}

#let currifyS0 = currifyS(2, currifyS)
#let flip(f) = (x, y) => f(y, x)

#let at_ = (x, y) => x.at(y)
#let at = currifyS0(2)(flip(at_))

#(/* position after */ at)
