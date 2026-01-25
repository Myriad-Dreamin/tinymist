/// path: u.typ

#let foo() = "foo"
#let bar() = "bar"

-----
/// path: main.typ
/// compile: true

#import "u.typ": bar as bbbar, foo

#let answer = 42
#answer
#bbbar()