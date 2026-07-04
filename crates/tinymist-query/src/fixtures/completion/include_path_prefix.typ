/// path: parts/section.typ
#let section = 1

-----
/// path: page.typ
#let page = 1

-----
/// contains: page.typ, parts/, parts/section.typ
#include "p"/* range -1..0 */
