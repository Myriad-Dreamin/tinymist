/// path: draw.typ

/// The draw line.
#let line() = 1;
-----
/// path: lib.typ

#import "draw.typ"
-----

#import "lib.typ"
#let draw = lib.draw;
#import draw: *
#(/* position after */ line);
