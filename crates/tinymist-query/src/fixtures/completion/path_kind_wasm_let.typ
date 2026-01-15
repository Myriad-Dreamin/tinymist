/// path: plugin.wasm
not-a-real-wasm

-----
/// path: base.typ
#let aa() = 1;

-----
/// contains: +plugin.wasm, -base.typ
#let p = ""/* range -1..0 */
#plugin(p)

