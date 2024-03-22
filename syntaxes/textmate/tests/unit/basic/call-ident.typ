#let f(..args) = f
#let g = (f: f)
#g.insert("g", g)

#f()
#f() []
#f() ()
#f ()
#f []
#f[] []

#(f())()
#( f())()
#(f() )()

#((f())(f()))
#f()()
#f(f)(f, f)
#(g.g.f)()

#list.item[]
#list[]

#{ f }()
#({ f })()
#{ f } ()
#({ f }) ()

#(list./*g*/item[])
#(list./*g*/ item[])
#(f /*g*/ ())


#{
  ("").join()
}

#test([#": "])
#{}
