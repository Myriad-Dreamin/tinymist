#let fun() = {
  return 2
}
#let foo(b: fun()) = b
#let x = foo()