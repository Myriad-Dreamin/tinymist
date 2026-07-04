/// path: a.typ
#import "b.typ": g

#let f(flag) = if flag {
  1
} else {
  g(true)
}

-----
/// path: b.typ
#import "a.typ": f

#let g(flag) = if flag {
  "done"
} else {
  f(true)
}

-----
/// path: main.typ
#import "a.typ": f
#import "b.typ": g

#let use-f() = f(false)
#let use-g() = g(false)
