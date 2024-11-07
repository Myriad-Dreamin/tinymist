/// path: base.typ
#let a(x) = a;
-----
#import "base.typ": *
#let f() = a(a)