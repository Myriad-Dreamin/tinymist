/// path: library.typ
#let inner() = panic("the value is invalid")
#let outer() = inner()
-----
#import "library.typ": outer

#outer()
