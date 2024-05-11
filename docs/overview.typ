#import "pageless.typ": *
#import "@preview/fletcher:0.4.4" as fletcher: *
#let colors = (blue.darken(10%), olive, eastern)
#import fletcher.shapes: diamond

#show: project.with(title: "Tinymist")

This document gives an overview of tinymist service, which provides a single integrated language service for Typst. This document doesn't dive in details but doesn't avoid showing code if necessary.

== Principles

Four principles are followed:

/ Multiple Actors: The main component, #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/crates/tinymist")[tinymist], starts as a thread or process, obeying the #link("https://microsoft.github.io/language-server-protocol/")[Language Server Protocol]. tinymist will bootstrap multiple actors, each of which provides some typst feature.

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
        node((2, +1), [Other Actors], fill: colors.at(1)),
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
  ) <fig:actor-serve-lsp-requests>

  A _Hover_ request is taken as the concrete example of that events.

  A global unique `LspActor` takes the event and mutates a global server state. If the event requires some additional code analysis, it is converted into an analysis request, #link("https://github.com/search?q=repo%3AMyriad-Dreamin/tinymist%20CompilerQueryRequest&type=code")[```rs struct CompilerQueryRequest```], and pushed to the actors owning compiler resources. Otherwise, `LspActor` responds to the event according to its state. Obviously, the _Hover_ on code request requires code analysis.

  The `CompileServerActor`s are created for each workspace and main entries (files/documents) in workspaces. When a compiler query is coming, a subset of that actors will take it and give project-specific responses, combining into a final concluded LSP response. Some analysis requests even require rendering features, and those requests will be pushed to the actors owning rendering resources. If you enable the periscope feature, a `Hover` on content request requires rendering on documents.

  The `RenderActor`s doesn't do compilations, but owning project-specific rendering cache and only design for rendering docuemnt in _low latency_. This is the last sinks of `Hover` requests. a `RenderActor` will receive an additional compiled `Document` object, and render the compiled frames in needed. After finishing rendering, a response attached with the rendered picture is sent to the LSP response channel intermediately.

/ Multi-level Analysis: The most critical features are lsp functions, built on the #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/crates/tinymist-query")[tinymist-query] crate. To achieve low latency, functions are classified into different levels of analysis.
  + `query_token_cache` – `TokenRequest` – locks and accesses token cache.
  + `query_source` – `SyntaxRequest` – locks and accesses a single source unit.
  + `query_world` – `SemanticRequest` – locks and accesses multiple source units.
  + `query_state` – `StatefulRequest` – acquires to accesses a specific version of compile results.

  todo: finish me.

  #diagram(
    node-stroke: 1pt,
    edge-stroke: 1pt,
    node((0, 0), [Start], corner-radius: 2pt, extrude: (0, 3)),
    edge("-|>"),
    node(
      (0, 1),
      align(center)[
        Hey, wait,\ this flowchart\ is a trap!
      ],
      shape: diamond,
    ),
    edge("d,r,u,l", "-|>", [Yes], label-pos: 0.1),
  )

*Optional Features* – All rest features in tinymist are *optional*. The significant features are enabled by default, but you can disable them with feature flags. For example, `tinymist` provides preview server features powered by `typst-preview`. The `preview` feature is optional, as it can be disabled totally on compiling the tinymist binary.

*Minimal Editor Frontends* – Leveraging the interface of LSP, tinymist provides frontends to each editor, located in the #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors")[editor folders]. They are minimal, meaning that LSP should finish its main LSP features as many as possible without help of editor frontends. The editor frontends just enhances your code experience. For example, the vscode frontend takes responsibility on providing some nice editor tools. It is recommended to install these editors frontend for your editors.

== Command System

== Compile Interrupts

== Query System

The query system takes a _stack scoped_ context object. Initially, the query system holds...

== Additional Concepts for Typst Language

=== Matchers

=== Definitions and Uses (References)

=== Type System
