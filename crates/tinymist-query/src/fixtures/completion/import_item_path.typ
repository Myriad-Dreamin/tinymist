/// path: base.typ
#let table-prefix() = 1;
-----
/// path: lib.typ
#import "base.typ"

-----
/// contains: table,table-prefix
#import "lib.typ": base.table/* range -1..1 */