/// path: refs.bib
@book{foo, title = {Foo}}

-----
/// path: refs.yaml
foo:
  type: book

-----
/// path: refs.yml
foo:
  type: book

-----
/// path: img.png
not-a-real-png

-----
/// contains: +refs.bib, +refs.yaml, +refs.yml, -img.png
#let p = ""/* range -1..0 */
#bibliography(p)
