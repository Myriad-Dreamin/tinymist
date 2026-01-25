/// path: u.typ

#let foo() = "foo"
#let bar() = "bar"

-----
/// path: main.typ
/// compile: true

#import "u.typ" as util: foo

#let value = foo()
#value


