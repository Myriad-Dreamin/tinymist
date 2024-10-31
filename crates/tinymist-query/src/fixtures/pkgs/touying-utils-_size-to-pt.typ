/// path: lib.typ
/// - level (auto, ): The level
#let _size-to-pt(size, container-dimension) = {
  let to-convert = size
  if type(size) == ratio {
    to-convert = container-dimension * size
  }
  measure(v(to-convert)).height
}
-----
/// contains: level, hierachical, depth
#import "lib.typ": *
#current-heading(/* range 0..1 */)[];
-----
/// contains: "body"
#import "lib.typ": *
#current-heading(level: /* range 0..1 */)[];
-----
/// contains: false, true
#import "lib.typ": *
#current-heading(hierachical: /* range 0..1 */)[];
-----
/// contains: false, true
#import "lib.typ": *
#current-heading(depth: /* range 0..1 */)[];
