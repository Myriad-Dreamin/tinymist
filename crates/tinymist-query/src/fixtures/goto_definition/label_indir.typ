// compile: true

#let test1(body) = figure(body)
#test1([Test1]) <fig:test1>
/* position after */ @fig:test1

#let test2(body) = test1(body)
#test2([Test2]) <fig:test2>
@fig:test2
