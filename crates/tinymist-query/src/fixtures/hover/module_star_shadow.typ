/// path: draw.typ

/// The draw line.
#let line() = 1;
-----
/// path: lib.typ

#import "draw.typ"
-----

#import "lib.typ"
#import lib.draw: *
#(/* position after */ line);
