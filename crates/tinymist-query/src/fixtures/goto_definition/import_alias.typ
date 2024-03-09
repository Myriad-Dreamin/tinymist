// path: base.typ
#let f() = 1;
-----
#import "base.typ": f as one
#(/* position after */ one);
