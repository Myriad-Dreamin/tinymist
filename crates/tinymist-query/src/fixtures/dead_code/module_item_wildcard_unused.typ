/// path: u.typ

#let foo() = "foo"
#let bar() = "bar"

-----
/// path: main.typ
/// compile: true

#import "u.typ": *

#let value = 42
#value

