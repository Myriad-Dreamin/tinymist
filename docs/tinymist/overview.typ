#import "mod.typ": *

#show: book-page.with(title: "Overview of Service")

This document gives an overview of tinymist service, which provides a single integrated language service for Typst. This document doesn't dive in details unless necessary.

== Principles

Four principles are followed, as detailed in #cross-link("/principles.typ")[Principles].

- Multiple Actors
- Multi-level Analysis
- Optional Non-LSP Features
- Minimal Editor Frontends

== Command System

The extra features are exposed via LSP's #link("https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#workspace_executeCommand")[`workspace/executeCommand`] request, forming a command system. They are detailed in #cross-link("/commands.typ")[Command System].

== Additional Concepts for Typst Language

=== AST Matchers

Many analyzers don't check AST node relationships directly. The AST matchers provide some indirect structure for analyzers.

- Most code checks the syntax object matched by `get_deref_target` or `get_check_target`.
- The folding range analyzer and def-use analyzer check the source file on the structure named _lexical hierarchy_.
- The type checker checks constraint collected by a trivial node-to-type converter.

=== Type System

Check #cross-link("/type-system.typ")[Type System] for more details.

== Notes on Implementing Language Features

Five basic analysis like _lexical hierarchy_, _def use info_ and _type check info_ are implemented first. And all rest Language features are implemented based on basic analysis. Check #cross-link("/analyses.typ")[Analyses] for more details.

