#let tmpl(content, font: none) = {
  set text(font: font)

  content
}

#tmpl(font: /* position after */ ("Test",))[]
