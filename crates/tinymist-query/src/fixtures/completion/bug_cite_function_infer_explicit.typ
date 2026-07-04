/// path: references.bib
@book{DBLP:books/lib/Knuth86a,
  author = {Donald E. Knuth},
  title = {The TeXbook},
  year = {1986},
  publisher = {Addison-Wesley}
}

-----
/// path: base.typ

#bibliography("references.bib")

-----
/// contains:DBLP:books/lib/Knuth86a,The TeXbook
/// compile: base.typ

#set heading(numbering: "1.1")

#let cite_prose(labl) = cite(labl)

#cite_prose(<DBLP> /* range -2..-1 */)
