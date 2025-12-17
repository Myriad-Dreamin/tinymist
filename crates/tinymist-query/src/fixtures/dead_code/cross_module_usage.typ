/// path: /main.typ

// Cross-module usage should count as a reference for dead code analysis.
#import "ieee.typ": ieee

#show: ieee

-----
/// path: /ieee.typ
/// compile: main.typ

#let ieee(body) = body
