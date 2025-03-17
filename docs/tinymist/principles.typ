#import "mod.typ": *

#show: book-page.with(title: [Principles])

Four principles are followed.

== Multiple Actors

The main component, #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/crates/tinymist")[tinymist], starts as a thread or process, obeying the #link("https://microsoft.github.io/language-server-protocol/")[Language Server Protocol]. tinymist will bootstrap multiple actors, each of which provides some typst feature.

The each actor holds and maintains some resources exclusively. For example, the compile server actor holds the well known ```rs trait World``` resource.

The actors communicate with each other by channels. An actor should own many receivers as its input, and many senders as output. The actor will take input from receivers _sequentially_. For example, when some LSP request or notification is coming as an LSP event, multiple actors serve the event collaboratively, as shown in @fig:actor-serve-lsp-requests.

#figure(
  align(
    center,
    diagram(
      edge-stroke: 0.85pt,
      node-corner-radius: 3pt,
      edge-corner-radius: 4pt,
      mark-scale: 80%,
      node((0, 0), [LSP Requests/\ Notifications\ (Channel)], fill: colors.at(0), shape: fletcher.shapes.hexagon),
      node((2, +1), [RenderActor], fill: colors.at(1)),
      node((2, 0), align(center)[`CompileServerActor`], fill: colors.at(1)),
      node((2, -1), [`LspActor` (Main Thread)], fill: colors.at(1)),
      node((4, 0), [LSP Responses\ (Channel)], fill: colors.at(2), shape: fletcher.shapes.hexagon),
      edge((0, 0), "r,u,r", "-}>"),
      edge((2, -1), "r,d,r", "-}>"),
      edge((2, 0), "rr", "-}>"),
      edge((2, 1), "r,u,r", "-}>"),
      edge((2, 0), (2, 1), align(center)[Rendering\ Requests], "-}>"),
      edge((2, -1), (2, 0), align(center)[Analysis\ Requests], "-}>"),
    ),
  ),
  caption: [The IO Graph of actors serving a LSP request or notification],
) <fig:actor-serve-lsp-requests>

A _Hover_ request is taken as example of that events.

A global unique `LspActor` takes the event and _mutates_ a global server state by the event. If the event requires some additional code analysis, it is converted into an analysis request, #link("https://github.com/search?q=repo%3AMyriad-Dreamin/tinymist%20CompilerQueryRequest&type=code")[```rs struct CompilerQueryRequest```], and pushed to the actors owning compiler resources. Otherwise, `LspActor` responds to the event directly. Obviously, the _Hover_ on code request requires code analysis.

The `CompileServerActor`s are created for workspaces and main entries (files/documents) in workspaces. When a compiler query is coming, a subset of that actors will take it and give project-specific responses, combining into a final concluded LSP response. Some analysis requests even require rendering features, and those requests will be pushed to the actors owning rendering resources. If you enable the periscope feature, a `Hover` on content request requires rendering on documents.

The `RenderActor`s don't do compilations, but own project-specific rendering cache. They are designed for rendering documents in _low latency_. This is the last sink of `Hover` requests. A `RenderActor` will receive an additional compiled `Document` object, and render the compiled frames in needed. After finishing rendering, a response attached with the rendered picture is sent to the LSP response channel intermediately.

== Multi-level Analysis

he most critical features are lsp functions, built on the #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/crates/tinymist-query")[tinymist-query] crate. To achieve higher concurrency, functions are classified into different levels of analysis.
+ `query_source` – `SyntaxRequest` – locks and accesses a single source unit.
+ `query_world` – `SemanticRequest` – locks and accesses multiple source units.
+ `query_state` – `StatefulRequest` – acquires to accesses a specific version of compile results.

When an analysis request is coming, tinymist _upgrades_ it to a suitable level as needed, as shown in @fig:analysis-upgrading-level. A higher level requires to hold more resources and takes longer time to prepare.

#let pg-node = node.with(corner-radius: 2pt, shape: "rect");
#figure(
  align(
    center,
    diagram(
      node-stroke: 1pt,
      edge-stroke: 1pt,
      edge("-|>", align(center)[Analysis\ Request], label-pos: 0.1),
      pg-node((1, 0), [Syntax\ Level]),
      edge("-|>", []),
      pg-node((3, 0), [Semantic\ Level]),
      edge("-|>"),
      pg-node((5, 0), [Stateful\ Level]),
      edge((5, 0), (6, 0), "-|>", align(center)[Analysis\ Response], label-pos: 1),
      for i in (1, 3, 5) {
        edge((i, 0), (i, -0.5), (5.5, -0.5), (5.6, 0), "-|>")
      },
      edge(
        (0.3, 0.4),
        (0.3, 0),
        "-|>",
        align(center)[clone #typst-func("Source")],
        label-anchor: "center",
        label-pos: -0.5,
      ),
      edge(
        (2, 0.4),
        (2, 0),
        "-|>",
        align(center)[snapshot ```rs trait World```],
        label-anchor: "center",
        label-pos: -0.5,
      ),
      edge(
        (4, 0.4),
        (4, 0),
        "-|>",
        align(center)[acquire #typst-func("Document")],
        label-anchor: "center",
        label-pos: -0.5,
      ),
    ),
  ),
  caption: [The analyzer upgrades the level to acquire necessary resources],
) <fig:analysis-upgrading-level>

== Optional Non-LSP Features

All non-LSP features in tinymist are *optional*. They are optional, as they can be disabled *totally* on compiling the tinymist binary. The significant features are enabled by default, but you can disable them with feature flags. For example, `tinymist` provides preview server features powered by `typst-preview`.

== Minimal Editor Frontends

Leveraging the interface of LSP, tinymist provides frontends to each editor, located in the #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors")[editor folders]. They are minimal, meaning that LSP should finish its main LSP features as many as possible without help of editor frontends. The editor frontends just enhances your code experience. For example, the vscode frontend takes responsibility on providing some nice editor tools. It is recommended to install these editors frontend for your editors.
