/// path: a.typ
#let foo = 1

-----
/// contains: +a.typ
#let p = ""/* range -1..0 */
#import p: foo

