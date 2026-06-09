/// path: a.typ
// dummy

-----
/// contains: +a.typ
#let (p, ..rest) = (""/* range -1..0 */,)
#include p

