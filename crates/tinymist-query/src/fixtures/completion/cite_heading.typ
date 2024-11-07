/// path: references.bib
@article{t,}

-----

/// contains:form,test
/// compile: true

#set heading(numbering: "1.1")

#let cite_prose(labl) = cite(labl)

= H <test>

#cite_prose(<t> /* range -3..-2 */)

#bibliography("references.bib")
