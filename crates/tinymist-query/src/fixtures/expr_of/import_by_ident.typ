// path: base.typ
#let f() = 1;
-----
// path: derive.typ
#import "base.typ"
-----
#import "derive.typ": *
#import base: *
#f()
