#import "mod.typ": *

#show: book-page.with(title: [Project Model])

This section documents the experimental project management feature. Implementation may change in future releases.

= The Core Configuration: `tinymist.projectResolution`

This setting controls how Tinymist resolves projects:

- *`singleFile` (Default)*:
  Treats each Typst file as an independent document (similar to Markdown workflows). No lock files or project caches are generated, which is suitable for most people who work with single Typst files or small projects.

- *`lockDatabase`*:
  Mimics Rust's project management by tracking compilation/preview history. Stores data in lock files (`tinymist.lock`) and cache directories, enabling automatic main file selection based on historical context.

= The Challenge to Handle Multiple-File Projects

When working with multiple-file projects, Tinymist faces the challenge of determining which file to use as the main file for compilation and preview. This is because:
+ All project files (entries, includes, imports) share `.typ` extensions.
+ No inherent distinction exists between entry files and dependencies.
+ Automatic detection is ambiguous without context.

This resembles the situation in C++, where the language server also struggles to determine the header files and source files in a project. In C++, the language servers and IDEs relies on the `compile_commands.json` file to understand the compilation context.

Inspired by C++ and Rust, we introduced the `lockDatabase` resolution method to relieve pain of handling multiple-file projects.

= The classic way: `singleFile`

This is the default resolution method and has been used for years. Despite using `singleFile`, you can still work with multiple files:
- The language server will serve the previewed file as the main file when previewing a file.
- Pinning a main file manually by commands is possible:
  - Use command `Typst Pin Main` (tinymist.pinMainToCurrent) to set the current file as the main file.
  - Use command `Typst Unpin Main` (tinymist.unpinMain) to unset the main file.

= A Sample Usage of `lockDatabase`

This feature is in early development stage, and may contain bugs or incomplete features. The following sample demonstrates how to use the `lockDatabase` resolution method. Here is the related #link("https://github.com/Myriad-Dreamin/tinymist/blob/5838c7d3005e6942b2b35b30ac93b9af6b8cf25a/editors/neovim/spec/lockfile_spec.lua")[test].

#let code-path(it) = it.text.split("/").map(raw).join("/" + sym.zws)

+ Set ```lua projectResolution = "lockDatabase"``` in LSP settings.
+ Like #link("https://github.com/Myriad-Dreamin/tinymist/blob/5838c7d3005e6942b2b35b30ac93b9af6b8cf25a/scripts/test-lock.sh")[#code-path(`scripts/test-lock.sh`)], compile a file using tinymist CLI with `--save-lock` flag: `tinymist compile --save-lock main.typ`. This will create a `tinymist.lock` file in the current directory, which contains the _Compilation History_ and project routes.
+ back to the editor, editing #link("https://github.com/Myriad-Dreamin/tinymist/blob/main/tests/workspaces/book/chapters/chapter1.typ")[#code-path(`chapters/chapter1.typ`)] will trigger PDF export of #link("https://github.com/Myriad-Dreamin/tinymist/blob/main/tests/workspaces/book/main.typ")[#code-path(`main.typ`)] automatically.

Please report issue on #link("https://github.com/Myriad-Dreamin/tinymist/issues")[GitHub] if you find any bugs, missing features, or have any questions about this feature.

= Stability Notice: `tinymist.lock`

We have been aware of backward compatibility issues, but any change of the schema of `tinymist.lock` may corrupt the `tinymist.lock` intentionally or unintentionally. The schema is unstable and in beta stage. You have to remove the `tinymist.lock` file to recovery from errors, and you could open an issue to discuss with us. To reliably keep compilation commands, please put `tinymist compile` commands in build system such as `Makefile` or `justfile`.

= Compilation History

The _Project Model_ only relies on the concept of _Compilation History_ and we will explain how it works and how to use.

The _Compilation History_ (`tinymist.lock`) is a set of records. Each record contains the following information about compilation:
- *Input Args*: Main file path, fonts, features (e.g., HTML/Paged as export target).
- *Export Args*: Output path, PDF standard to use.

The source of _Compilation History_:
- (Implemented) CLI commands: `tinymist compile/preview --save-lock`, suitable for all the editor clients.
- (Not Implemented) LSP commands: `tinymist.exportXXX`/`previewXXX`, suitable for vscode or neovim clients, which allows client-side extension.
- (Not Implemented) External tools: Tools that update the lock file directly. For example, the tytanic could update all example documents once executed test commands. The official typst could also do this to tell whether a test case is compiled targeting HTML or PDF.

== Utilizing Compilation History

There are several features that can be implemented using the _Compilation History_:

- Correct Entry Detection: When a user runs the compile or preview commands, Tinymist will save the _Compilation History_ to the lock file and the language server will identify the main file correctly.
- Dynamic Entry Switching: When a user runs another compile command, the newer command will have higher priorit, and Tinymist will "switch" the main file accordingly.
- Per-Document Flags: Some documents are compiled to HTML, while others are compiled to PDF. The users can specify more compile flags for each document.
- Session Persistence: Users can open the recently previewed file and continue editing it. More state such as the scroll position could be remembered in future.
- Sharing and VCS: The lock file can be shared with other users by tools like git, allowing them to compile the same project with the same settings.

== Storing Compilation History

- *Storage in file system*: stored in `tinymist.lock` (TOML) files. When resolving a depended file, the nearest lock file will be used to determine the compilation arguments.
- *Storage in memory*: The language server also maintains a _Compilation History_ and project routes in memory. We may enable in-memory _Compilation History_ by default in the future, which will allow Tinymist to resolve projects smarter.

= Project Route

The language server will load route entries from disk or memory, combine, and perform entry lookup based on the route table. Specificially, *The depended files* of a single compilation will be stored as route entries in the cache directory after compilation. A single route entry is a triple, (Dependent Path, Project ID, Priority), where:
- "Dependent Path" is an absoulte dependent file path, like paths to assets and source files in packages.
- "Project ID" is the project id (main file) indexing an entry in the _Compilation History_ (`tinymist.lock`).
- "Priority" is a priority number.

And the language server determines a project id associating some dependent file by the following rules:

+ Highest priority routes take precedence.
+ Most recent updated projects in _Compilation History_ prioritized automatically.

The cache directory contains cache of project routes. Currently, we haven't implemented a way clean up or garbage collect the project route cache, and disk cache may be deprecated in future. It is safe to remove all the project routes in the cache directory, as Tinymist will regenerate them when needed.
