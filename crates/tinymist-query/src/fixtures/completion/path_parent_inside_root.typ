/// path: base.typ
#let aa = 1

-----
/// path: nested/main.typ
/// contains: ../, ../base.typ
#read("../"/* range -4..0 */)
