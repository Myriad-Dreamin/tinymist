/// path: u.typ

#let foo() = "foo"

-----
/// path: main.typ
/// compile: true

#import "u.typ": *
#import "u.typ": *

#let value = foo()
#value
