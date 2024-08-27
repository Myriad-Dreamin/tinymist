#import "@preview/shiroa:0.1.1": *

#import "/typ/templates/ebook.typ"

#show: ebook.project.with(title: "tinymist", spec: "book.typ")

// set a resolver for inclusion
#ebook.resolve-inclusion(it => include it)
