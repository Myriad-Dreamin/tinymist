// path: base.typ
#let tmpl(content, font: none) = {
  set text(font: font)

  content
}
-----
#import "base.typ": *

#tmpl(font: /* position after */ ("Test",))[]
