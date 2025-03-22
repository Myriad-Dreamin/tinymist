/// path: baka.typ
#import "b.typ" as baka
#import baka: *

-----
/// path: b.typ

#let baka(body) = body

-----
/// contains: baka
#import "baka.typ": *

#show: ba/* range -1..0 */
