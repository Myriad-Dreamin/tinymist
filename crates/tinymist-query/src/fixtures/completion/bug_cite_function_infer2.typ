// contains: test

#set heading(numbering: "1.1")

#let cite_prose(labl) = ref(labl)
#let cite_prose_different_name(labl) = ref(labl)

= Test <test>

#cite_prose_different_name( /* range -1..0 */)
