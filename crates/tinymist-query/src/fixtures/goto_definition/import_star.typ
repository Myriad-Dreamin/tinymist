// path: base.typ
#let f() = 1;
-----
.

#import "base.typ": *
#(/* position after */ f);
