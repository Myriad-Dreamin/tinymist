/// path: refs.bib
@book{foo, title = {Foo}}

-----
/// path: style.csl
not-a-real-csl

-----
/// path: other.csl
not-a-real-csl

-----
/// contains: +style.csl, +other.csl
#let styles = (""/* range -1..0 */,)
#for s in styles {
  bibliography("refs.bib", style: s)
}

