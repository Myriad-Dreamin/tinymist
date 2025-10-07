/// path: empty.typ
-----
/// path: a.typ
#let a = 0
#import "empty.typ": *
#let a = 1
-----
#import "a.typ": /* ident after */ a
