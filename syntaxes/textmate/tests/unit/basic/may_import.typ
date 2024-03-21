#let evil_import() = import "base.typ"
#import "base.typ"
#import ("base.typ")
#import("base.typ")
#import "base.typ": *
#import "base.typ" as t: *
#import "base.typ": x
#import "base.typ": x as foo
#import "base.typ": x as foo, y
#import "base.typ" as foo
#import "base.typ" as foo: z, x as foo, y as t
#import cetz.draw: *
