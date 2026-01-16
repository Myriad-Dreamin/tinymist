/// path: refs.bib
@book{foo, title = {Foo}}

-----
/// path: style.csl
not-a-real-csl

-----
/// path: data.yaml
a: 1

-----
/// contains: +style.csl, -data.yaml
#let p = ""/* range -1..0 */
#bibliography("refs.bib", style: p)

