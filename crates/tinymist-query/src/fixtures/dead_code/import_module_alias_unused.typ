/// path: u.typ

#let foo() = "foo"
#let bar() = "bar"

-----
/// path: main.typ
/// compile: true

#import "u.typ" as util

#let answer = 42
#answer


