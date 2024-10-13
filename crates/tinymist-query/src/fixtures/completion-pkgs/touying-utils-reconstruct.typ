// path: lib.typ
#let reconstruct(body-name: "body", labeled: true, named: false, it, ..new-body) = { }
-----
// contains: body-name, labeled, named, it, new-body
#import "lib.typ": *
#reconstruct(/* range 0..1 */)[];
-----
// contains: "body"
#import "lib.typ": *
#reconstruct(body-name: /* range 0..1 */)[];
-----
// contains: false, true
#import "lib.typ": *
#reconstruct(labeled: /* range 0..1 */)[];
-----
// contains: false, true
#import "lib.typ": *
#reconstruct(named: /* range 0..1 */)[];
