/// path: u.typ

#let foo() = "foo"
#let bar() = "bar"

-----
/// path: main.typ
/// compile: true

#import "u.typ": bar as Bbar, foo

#let answer = 42
#answer

