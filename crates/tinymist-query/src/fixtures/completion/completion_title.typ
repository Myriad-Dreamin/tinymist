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

#set heading(numbering: "1.")
== Test <R>
@R/* range -2..0 */

#bibliography("references.bib")
