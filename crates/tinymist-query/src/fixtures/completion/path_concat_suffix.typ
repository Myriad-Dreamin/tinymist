/// path: a.typ
// dummy

-----
/// path: b.typ
// dummy

-----
/// contains: +a.typ, +b.typ
#include ""/* range -1..0 */ + ".typ"

