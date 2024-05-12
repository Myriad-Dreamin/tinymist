#import "pageless.typ": *
#import "@preview/fletcher:0.4.4" as fletcher: *
#let colors = (blue.darken(10%), olive, eastern)
#import fletcher.shapes: diamond

#show: project.with(title: "Tinymist")

/// This function is to render a text string in monospace style and function
/// color in your defining themes.
///
/// ## Examples
///
/// ```typc
/// typst-func("list.item")
/// ```
///
/// Note: it doesn't check whether input is a valid function identifier or path.
#let typst-func(it) = [
  #raw(it + "()", lang: "typc") <typst-raw-func>
]

#show <typst-raw-func>: it => {
  it.lines.at(0).body.children.slice(0, -2).join()
}

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
    caption: [The I-O Graph of actors serving a LSP request or notification],
  ) <fig:actor-serve-lsp-requests>

  A _Hover_ request is taken as example of that events.

  A global unique `LspActor` takes the event and mutates a global server state. If the event requires some additional code analysis, it is converted into an analysis request, #link("https://github.com/search?q=repo%3AMyriad-Dreamin/tinymist%20CompilerQueryRequest&type=code")[```rs struct CompilerQueryRequest```], and pushed to the actors owning compiler resources. Otherwise, `LspActor` responds to the event according to its state. Obviously, the _Hover_ on code request requires code analysis.

  The `CompileServerActor`s are created for each workspace and main entries (files/documents) in workspaces. When a compiler query is coming, a subset of that actors will take it and give project-specific responses, combining into a final concluded LSP response. Some analysis requests even require rendering features, and those requests will be pushed to the actors owning rendering resources. If you enable the periscope feature, a `Hover` on content request requires rendering on documents.

  The `RenderActor`s don't do compilations, but own project-specific rendering cache. They are designed for rendering docuemnt in _low latency_. This is the last sink of `Hover` requests. A `RenderActor` will receive an additional compiled `Document` object, and render the compiled frames in needed. After finishing rendering, a response attached with the rendered picture is sent to the LSP response channel intermediately.

/ Multi-level Analysis: The most critical features are lsp functions, built on the #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/crates/tinymist-query")[tinymist-query] crate. To achieve low latency, functions are classified into different levels of analysis.
  // + `query_token_cache` – `TokenRequest` – locks and accesses token cache.
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

/ Optional Non-LSP Features: All non-LSP features in tinymist are *optional*. They are optional, as they can be disabled *totally* on compiling the tinymist binary. The significant features are enabled by default, but you can disable them with feature flags. For example, `tinymist` provides preview server features powered by `typst-preview`.

/ Minimal Editor Frontends: Leveraging the interface of LSP, tinymist provides frontends to each editor, located in the #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors")[editor folders]. They are minimal, meaning that LSP should finish its main LSP features as many as possible without help of editor frontends. The editor frontends just enhances your code experience. For example, the vscode frontend takes responsibility on providing some nice editor tools. It is recommended to install these editors frontend for your editors.

== Command System

The extra features are exposed via LSP's #link("https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#workspace_executeCommand")[`workspace/executeCommand`] request, forming a command system. The commands in the system have a name convention.

- `export`#text(olive, `Fmt`). these commands perform export on some document, with a specific format (#text(olive, `Fmt`)), e.g. `exportPdf`.

- `interactCodeContext({`#text(olive, `kind`)`}[])`. The code context requests are useful for _Editor Frontends_ to extend some semantic actions. A batch of requests are sent at the same time, to get code context _atomically_.

- `getResources(`#text(olive, `"path/to/resource/"`)`, `#text(red, `opts`)`)`. The resources required by _Editor Frontends_ should be arranged in #text(olive, "paths"). A second arguments can be passed as options to request a resource. This resemebles a restful `POST` action to LSP, with a url #text(olive, "path") and a HTTP #text(red, "body"), or a RPC with a #text(olive, "method name") and #text(red, "params").

  Note you can also hide some commands in list of commands in UI by putting them in `getResources` command.

- `do`#text(olive, `Xxx`). these commands are internally for _Editor Frontends_, and you'd better not to invoke them directly. You can still invoke them manually, as long as you know what would happen.

- The rest commands are public and tend to be user-friendly.

// === Stateful Commands

// Two styles are made for stateful commands.

=== Code Context

The code context requests are useful for _Editor Frontends_ to check syntax and semantic the multiple positions. For example an editor frontend can filter some completion list by acquire the code context at current position.

== Replayable Input Events

The compilation triggers many side effects, but compiler actor is still almost pure. This is achieved by accepting all compile inputs by events.

Let us take reading files from physical file system as example of processing compile inputs, as shown in @fig:overlay-vfs.

#let pg-hori-sep = 1.5
#let pg-vert-sep = 0.7
#let pg-adjust = 18pt
#figure(
  align(
    center,
    move(
      dx: pg-adjust,
      diagram(
        edge-stroke: 0.85pt,
        node-corner-radius: 3pt,
        edge-corner-radius: 4pt,
        mark-scale: 80%,
        node((pg-hori-sep, +pg-vert-sep), [SystemAccessModel], fill: colors.at(1)),
        node((pg-hori-sep, 0), align(center)[`NotifyAccessModel`], fill: colors.at(1)),
        node((pg-hori-sep, -pg-vert-sep), [MemoryAccessModel], fill: colors.at(1)),
        node((0, 0), align(center)[`NotifyActor`], fill: colors.at(0)),
        node((0, -pg-vert-sep), align(center)[`LspActor`], fill: colors.at(0)),
        edge((0, 0), (pg-hori-sep, 0), "-}>"),
        edge((0, -pg-vert-sep), (pg-hori-sep, -pg-vert-sep), "-}>"),
        edge(
          (-1, -pg-vert-sep),
          (0, -pg-vert-sep),
          "-}>",
          [didChange, \ didOpen, etc.],
          label-anchor: "center",
          label-pos: 0,
        ),
        edge(
          (-0.8, pg-vert-sep),
          (0, pg-vert-sep),
          (0, 0),
          "-}>",
          [readFile\ readDir, etc.],
          label-anchor: "center",
          label-pos: 0,
        ),
        edge((-1, pg-vert-sep), (pg-hori-sep, pg-vert-sep), "-}>"),
        edge((pg-hori-sep, 0), (pg-hori-sep, pg-vert-sep), "-}>"),
        edge((pg-hori-sep, -pg-vert-sep), (pg-hori-sep, 0), "-}>"),
        edge(
          (pg-hori-sep * 1.59, -pg-vert-sep * 1.6),
          (pg-hori-sep, -pg-vert-sep * 1.6),
          (pg-hori-sep, -pg-vert-sep),
          "-}>",
          [sourceOf(path)],
          label-pos: 0.2,
        ),
        for i in (-1, 0, 1) {
          edge(
            (pg-hori-sep * 1.2, i * pg-vert-sep),
            (pg-hori-sep * 1.7, i * pg-vert-sep),
            "-}>",
            [source],
            label-pos: 1,
          )
        },
        node(
          (-1.3, 0),
          rotate(-90deg, rect(stroke: (bottom: (thickness: 1pt, dash: "dashed")), width: 120pt)[Side Effects]),
        ),
        node(
          (pg-hori-sep + 1.45, 0),
          rotate(
            90deg,
            move(
              dy: pg-adjust * 2,
              rect(stroke: (bottom: (thickness: 1pt, dash: "dashed")), width: 120pt)[Compiler World],
            ),
          ),
        ),
      ),
    ),
  ),
  caption: [The overlay virtual file system (VFS)],
) <fig:overlay-vfs>

== Query System

The query system takes a _stack scoped_ context object. Initially, the query system holds...

== Additional Concepts for Typst Language

=== Matchers

=== Definitions and Uses (References)

=== Type System
