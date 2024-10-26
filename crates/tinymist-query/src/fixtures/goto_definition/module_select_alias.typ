// path: variable.typ
#let f(x) = 2;
-----

#import "variable.typ" as this-module
#(this-module.f /* position after */ );
