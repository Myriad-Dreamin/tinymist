/// path: base.typ
#let collect-rest(..items) = {
  let result = ()
  for item in items.pos() {
    result.push(item)
  }
  (none, ..result)
}
-----
#import "base.typ": *

#let (_, collected) = collect-rest("external")
#let (_, number) = collect-rest(1)
