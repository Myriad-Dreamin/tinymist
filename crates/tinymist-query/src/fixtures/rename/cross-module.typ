// path: user.typ
#let f() = 1;
-----
#import "user.typ": f
#(/* position after */ f);
