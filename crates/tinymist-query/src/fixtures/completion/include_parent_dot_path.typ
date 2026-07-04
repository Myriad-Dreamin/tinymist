/// path: nanochat/parts/section.typ
#let section = 1

-----
/// path: nanochat/main.typ
#let main = 1

-----
/// path: mira/main.typ
/// contains: ../nanochat/main.typ, ../nanochat/parts/, ../nanochat/parts/section.typ
#include "../nanochat/."/* range -1..0 */
