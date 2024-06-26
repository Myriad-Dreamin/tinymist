#import "mod.typ": *

#show: book-page.with(title: "Tinymist Type System")

The underlying techniques are not easy to understand, but there are some links:
- bidirectional type checking: https://jaked.org/blog/2021-09-15-Reconstructing-TypeScript-part-1
- type system borrowed here: https://github.com/hkust-taco/mlscript

Some tricks are taken for help reducing the complexity of code:

First, the array literals are identified as tuple type, that each cell of the array has type individually.

#let sig = $sans("sig")$
#let ags = $sans("args")$

Second, the $sig$ and the $sans("argument")$ type are reused frequently.

- the $sans("tup")$ type is notated as $(tau_1,..,tau_n)$, and the $sans("arr")$ type is a special tuple type $sans("arr") ::= sans("arr")(tau)$.

- the $sans("rec")$ type is imported from #link("https://github.com/hkust-taco/mlscript")[mlscript], notated as ${a_1=tau_1,..,a_n=tau_n}$.

- the $sig$ type consists of:
  - a positional argument list, in $sans("tup")$ type.
  - a named argument list, in $sans("rec")$ type.
  - an optional rest argument, in $sans("arr")$ type.
  - an *optional* body, in any type.

  notated as $sig := sig(sans("tup")(tau_1,..,tau_n),sans("rec")(a_1=tau_(n+1),..,a_m=tau_(n+m)),..sans("arr")(tau_(n+m+1))) arrow psi$
- the $sans("argument")$ is a $sans("signature")$ without rest and body.

  $ags := ags(sig(..))$

With aboving constructors, we soonly get typst's type checker.

- it checks array or dictionary literals by converting them with a corresponding $sig$ and $ags$.
- it performs the getting element operation by calls a corresponding $sig$.
- the closure is converted into a typed lambda, in $sig$ type.
- the pattern destructing are converted to array and dictionary constrains.
