/// path: u.typ

#let foo() = "foo"

-----
/// path: main.typ
/// compile: true

#import "u.typ" as a
#import "u.typ" as b

#let value = a.foo()
#value
