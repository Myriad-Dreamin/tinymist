// path: lib.typ
// - level (auto, ): The level
#let fit-to-height(
  width: none,
  prescale-width: none,
  grow: true,
  shrink: true,
  height,
  body,
) = {
  [
    #show before-label: none
    #show after-label: none
    #v(1em)
    hidden#before-label
    #v(height)
    hidden#after-label
  ]
}
-----
// contains: 1
#import "lib.typ": *
#fit-to-height(width: /* range 0..1 */)[];
-----
// contains: 1
#import "lib.typ": *
#fit-to-height(prescale-width: /* range 0..1 */)[];
-----
// contains: 1
#import "lib.typ": *
#fit-to-height(height: /* range 0..1 */)[];
