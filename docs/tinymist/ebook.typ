#import "@preview/shiroa:0.2.2": *

#import "/typ/templates/ebook.typ"

#import "/typ/templates/tinymist-version.typ": tinymist-package

#show: ebook.project.with(title: [Tinymist Documentation (v#tinymist-package.version)], spec: "book.typ")

// set a resolver for inclusion
#ebook.resolve-inclusion(it => include it)
