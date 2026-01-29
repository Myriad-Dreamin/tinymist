/// path: a.typ
// dummy

-----
/// contains: +a.typ
#let files = (""/* range -1..0 */,)
#for file in files {
  include file
}
