// path: base.typ
#let x = 1;
#x
-----
// path: base2.typ
#import "base.typ": *
#let y = 2;
-----
#import "base2.typ": *
#x, #y