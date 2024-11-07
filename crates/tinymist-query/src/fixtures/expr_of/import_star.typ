/// path: base.typ
#let x = 1;
#x
-----
#import "base.typ"
#base
#import "base.typ": *
#base, #x