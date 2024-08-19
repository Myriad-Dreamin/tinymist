// path: base2.typ
== Base 2 <b2>
Ref to b1 @b1
Ref to b2 @b2
Ref to b1 @b1 again
-----
// path: base1.typ
== Base 1 <b1>
Ref to b1 @b1
Ref to b2 @b2
-----
// compile:true

#set heading(numbering: "1.")
= Test Ref Label
#include "base1.typ"
#include "base2.typ"
Ref to b1 /* position after */ @b1
Ref to b2 @b2