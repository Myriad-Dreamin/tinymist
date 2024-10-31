/// path: variable.typ
#let x = 2;
-----
.

#import "variable.typ": *
#(/* position after */ x);
