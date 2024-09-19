// path: references.bib
@article{Russell:1908,
Author = {Bertand Russell},
Journal = {American Journal of Mathematics},
Pages = {222--262},
Title = {Mathematical logic based on the theory of types},
Volume = 30,
Year = 1908}

-----
// contains:Russell:1908,Mathematical logic based on the theory of types
// compile:true

#set heading(numbering: "1.1")

#let cite_prose(labl) = ref(labl)
#let cite_prose_different_name(labl) = ref(labl)

= Test <Rus>

#bibliography("references.bib")

#cite_prose(<Rus> /* range -2..-1 */)
