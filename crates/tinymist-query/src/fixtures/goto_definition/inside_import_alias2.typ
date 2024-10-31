/// path: base.typ
#let f() = 1;
-----
.

#import "base.typ": f as /* ident after */ ff;
