/// path: base.typ
#let f() = 1;
-----
#import "base.typ": f
#(/* position after */ f);
