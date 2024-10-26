#import "mod.typ": *

#show: book-page.with(title: "Tinymist Languague Queries")

== Base Analyses

There are seven basic analyzers:
- _lexical hierarchy_ matches crucial lexical structures in the source file.
- _expression info_ is computed incrementally on source files.
- _type check info_ is computed with _expression info_.
- _definition finder_ finds the definition based on _expression info_ and _type check info_.
- _references finder_ finds the references based on _definitions_ and _expression info_.
- _signature resolver_ summarizes signature based on _definitions_ and _type check info_.
- _call resolver_ check calls based on _signatures_ and _type check info_.

#let pg-node = node.with(corner-radius: 2pt, shape: "rect");
#let ref-n = (0.6, 1);
#let def-n = (1.9, 1);
#let sig-n = (3.1, 1);
#let call-n = (4.3, 1);
#let expr-n = (2.2, 0);
#let type-n = (3.3, 0);
#figure(
  align(
    center,
    diagram(
      node-stroke: 1pt,
      edge-stroke: 1pt,
      // edge("-|>", align(center)[Analysis\ Request], label-pos: 0.1),
      pg-node((0.3, 0.2), [`Lexical`\ `Heirarchy`]),
      edge("<|-", []),
      pg-node((1.2, 0), [`Source`]),
      edge("-|>", []),
      pg-node(expr-n, [`ExprInfo`]),
      edge("-|>"),
      pg-node(type-n, [`TypeInfo`]),
      edge("-|>"),
      pg-node(def-n, [`Definition`]),
      edge(expr-n, def-n, "-|>"),
      pg-node(ref-n, [`References`]),
      edge(def-n, ref-n, "-|>"),
      edge(expr-n, ref-n, "-|>"),
      pg-node(sig-n, [`Signature`]),
      edge(def-n, sig-n, "-|>"),
      edge(type-n, sig-n, "-|>"),
      pg-node(call-n, [`Call`]),
      edge(sig-n, call-n, "-|>"),
      edge(type-n, call-n, "-|>"),
      for i in range(9) {
        let j = 1 + i * 0.25;
        edge((j, 1.4), (j, 1.8), "-|>")
      },
      pg-node((2, 2.3), [`Extented`\
      `Language Features`]),
      // for i in (1, 3, 5) {
      //   edge((i, 0), (i, -0.5), (5.5, -0.5), (5.6, 0), "-|>")
      // },
      // edge(
      //   (0.3, 0.4),
      //   (0.3, 0),
      //   "-|>",
      //   align(center)[clone #typst-func("Source")],
      //   label-anchor: "center",
      //   label-pos: -0.5,
      // ),
      // edge(
      //   (2, 0.4),
      //   (2, 0),
      //   "-|>",
      //   align(center)[snapshot ```rs trait World```],
      //   label-anchor: "center",
      //   label-pos: -0.5,
      // ),
      // edge(
      //   (4, 0.4),
      //   (4, 0),
      //   "-|>",
      //   align(center)[acquire #typst-func("Document")],
      //   label-anchor: "center",
      //   label-pos: -0.5,
      // ),
    ),
  ),
  caption: [The relationship of analyzers.],
) <fig:analyses-relationship>

== Extending Language Features

Typicial language features are implemented based on basic analyzers:

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
