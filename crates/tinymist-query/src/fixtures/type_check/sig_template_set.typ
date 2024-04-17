#let tmpl(content, authors: (), font: none, class: "article") = {
  if class != "article" or class != "letter" {
    panic("")
  }

  set document(author: authors)
  set text(font: font)

  set page(paper: "a4") if class == "article"
  set page(paper: "us-letter") if class == "letter"

  content
}
