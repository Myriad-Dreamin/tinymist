#import "@preview/shiroa:0.2.3": *

#import "/typ/templates/ebook.typ"

#import "/typ/templates/tinymist-version.typ": tinymist-package

#show: ebook.project.with(title: [Tinymist Documentation (v#tinymist-package.version)], spec: "book.typ")

#external-book(spec: include "/docs/tinymist/book.typ")

// set a resolver for inclusion
#ebook.resolve-inclusion(it => include it)
