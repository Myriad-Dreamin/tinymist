/// path: base.typ
#let f() = 1;
-----
.

#import "/base.typ": *;

#let conf() = {
  import "@preview/example:0.1.0";

  set text(size: /* ident after */ f());
}
