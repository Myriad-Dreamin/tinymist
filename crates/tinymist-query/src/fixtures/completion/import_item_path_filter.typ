/// path: base.typ
#let table-prefix() = 1;
#let table-prefix2() = 1;
-----
/// path: lib.typ
#import "base.typ"

-----
/// contains: table,table-prefix,table-prefix2
#import "lib.typ": base.table-prefix, base.table/* range -1..1 */