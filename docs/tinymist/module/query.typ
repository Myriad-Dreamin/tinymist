#import "mod.typ": *

#show: book-page.with(title: "Tinymist Languague Queries")

== Base Analyses

There are five basic analysis APIs:
- _lexical hierarchy_ matches crucial lexical structures in the source file.
- _def use info_ is computed based on _lexical hierarchy_\s.
- _type check info_ is computed with _def use info_.
- _find definition_ finds the definition based on _def use info_.
- _find references_ finds the references based on _def use info_.

== Extending Language Features

Language features are implemented based on basic analysis APIs:

- The `textDocument/documentSymbol` returns a tree of nodes converted from the _lexical hierarchy_.

- The `textDocument/foldingRange` also returns a tree of nodes converted from the _lexical hierarchy_ but with a different approach.

- The `workspace/symbol` returns an array of nodes converted from all _lexical hierarchy_\s in workspace.

- The `textDocument/definition` returns the result of _find definition_.

- The `textDocument/completion` returns a list of types of _related_ nodes according to _type check info_, matched by _AST matchers_.

- The `textDocument/hover` _finds definition_ and prints the definition with a checked type by _type check info_. Or, specific to typst, prints a set of inspected values during execution of the document.

- The `textDocument/signatureHelp` also _finds definition_ and prints the signature with union of inferred signatures by _type check info_.

- The `textDocument/prepareRename` _finds definition_ and determines whether it can be renamed.

- The `textDocument/rename` _finds defintion and references_ and renamed them all.

== Contributing

See #link("https://github.com/Myriad-Dreamin/tinymist/blob/main/CONTRIBUTING.md")[CONTRIBUTING.md].
