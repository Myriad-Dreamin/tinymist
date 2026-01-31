/// path: dir/a.typ
// dummy

-----
/// path: a.typ
// dummy

-----
/// contains: +dir/a.typ, -a.typ
#let dir = "dir/"
#include dir + ""/* range -1..0 */

