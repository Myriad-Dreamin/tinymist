/// path: u.typ

#let foo() = "foo"
#let bar() = "bar"

-----
/// path: main.typ
/// compile: true

#import "u.typ"

#let answer = 42
#answer
