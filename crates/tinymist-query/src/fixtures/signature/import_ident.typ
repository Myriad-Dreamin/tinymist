// path: base.typ
#let f(u, v) = u + v;
-----
#import "base.typ": f
#(/* ident after */ f);
