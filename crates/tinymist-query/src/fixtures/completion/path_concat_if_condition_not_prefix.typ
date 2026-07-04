/// path: dir/a.typ
// dummy

-----
/// path: a.typ
// dummy

-----
/// contains: a.typ, dir/a.typ
#let prefix = if "dir/" == "dir/" { none } else { none }
#include prefix + ""/* range -1..0 */
