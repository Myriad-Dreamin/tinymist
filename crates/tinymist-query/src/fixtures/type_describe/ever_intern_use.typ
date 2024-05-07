#let tmpl(content) = {
  content = 1
  content = 2
  content = 3
  content
}

#(tmpl(4))

#(/* position after */ tmpl)
