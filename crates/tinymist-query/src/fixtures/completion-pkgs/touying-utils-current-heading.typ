// path: lib.typ
// - level (auto, int): The level
#let current-heading(level: auto, hierachical: true, depth: 9999) = {
  let current-page = here().page()
  if not hierachical and level != auto {
    let headings = query(heading).filter(h => (
      h.location().page() <= current-page and h.level <= depth and h.level == level
    ))
    return headings.at(-1, default: none)
  }
  let headings = query(heading).filter(h => h.location().page() <= current-page and h.level <= depth)
  if headings == () {
    return
  }
  if level == auto {
    return headings.last()
  }
  let current-level = headings.last().level
  let current-heading = headings.pop()
  while headings.len() > 0 and level < current-level {
    current-level = headings.last().level
    current-heading = headings.pop()
  }
  if level == current-level {
    return current-heading
  }
}
-----
// contains: level, hierachical, depth
#import "lib.typ": *
#current-heading(/* range 0..1 */)[];
-----
// contains: "body"
#import "lib.typ": *
#current-heading(level: /* range 0..1 */)[];
-----
// contains: false, true
#import "lib.typ": *
#current-heading(hierachical: /* range 0..1 */)[];
-----
// contains: 9999, 1
#import "lib.typ": *
#current-heading(depth: /* range 0..1 */)[];
