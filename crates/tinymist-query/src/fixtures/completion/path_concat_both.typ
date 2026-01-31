/// path: dir/a.typ
// dummy

-----
/// path: dir/b.typ
// dummy

-----
/// path: dir/c.txt
// dummy

-----
/// contains: +dir/a.typ, +dir/b.typ, -dir/c.txt
#include "dir/" + ""/* range -1..0 */ + ".typ"

