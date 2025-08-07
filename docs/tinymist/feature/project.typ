#import "mod.typ": *

#show: book-page.with(title: [Project Model])

This section documents the experimental project management feature. Implementation may change in future releases.

= The Core Configuration: `tinymist.projectResolution`

This setting controls how Tinymist resolves projects:

- *`singleFile` (Default)*:
  Treats each Typst file as an independent document (similar to Markdown workflows). No lock files or project caches are generated, which is suitable for most people who work with single Typst files or small projects.

- *`lockDatabase`*:
  Mimics Rust's project management by tracking compilation/preview history. Stores data in lock files and cache directories, enabling automatic main file selection based on historical context.

= The Challenge to Handle Multiple-File Projects

When working with multiple-file projects, Tinymist faces the challenge of determining which file to use as the main file for compilation and preview. This is because:
+ All project files (entries, includes, imports) share `.typ` extensions
+ No inherent distinction exists between entry files and dependencies
+ Automatic detection is ambiguous without context

This resembles the situation in C++, where the language server also struggles to determine the header files and source files in a project. In C++, the language servers and IDEs relies on the `compile_commands.json` file to understand the compilation context.

Inspired by C++ and Rust, we introduced the `lockDatabase` resolution method to relieve pain of handling multiple-file projects.

= A Sample Usage of `lockDatabase`

This feature is in early development stage, and may contain bugs or incomplete features. The following sample demonstrates how to use the `lockDatabase` resolution method. Here is the related #link("https://github.com/Myriad-Dreamin/tinymist/blob/5838c7d3005e6942b2b35b30ac93b9af6b8cf25a/editors/neovim/spec/lockfile_spec.lua")[test].

#let code-path(it) = it.text.split("/").map(raw).join(sym.zws)

+ Set ```lua projectResolution = "lockDatabase"``` in `~/.config/tinymist/config.toml`
+ Like #link("https://github.com/Myriad-Dreamin/tinymist/blob/5838c7d3005e6942b2b35b30ac93b9af6b8cf25a/scripts/test-lock.sh")[#code-path(`scripts/test-lock.sh`)], compile a file using tinymist CLI with `--save-lock` flag: `tinymist compile --save-lock main.typ`. This will create a `tinymist.lock` file in the current directory, which contains the compilation history and project routes.
+ back to the editor, editing #link("https://github.com/Myriad-Dreamin/tinymist/blob/main/tests/workspaces/book/chapters/chapter1.typ")[#code-path(`chapters/chapter1.typ`)] will trigger PDF export of #link("https://github.com/Myriad-Dreamin/tinymist/blob/main/tests/workspaces/book/main.typ")[#code-path(`main.typ`)] automatically.

Please report issue on #link("https://github.com/Myriad-Dreamin/tinymist/issues")[GitHub] if you find any bugs, missing features, or have any questions about this feature.

= Compilation History

The compilation history (`tinymist.lock`) is a set of records. Each record contains the following information about compilation:
- *Input Args*: Main file path, fonts, features (e.g., HTML/Paged as export target).
- *Export Args*: Output path, PDF standard to use.
- *Dependencies*: Source files, assets.

The source of compilation history:
- (Implemented) CLI commands: `tinymist compile/preview --save-lock`, suitable for all the editor clients.
- (Not Implemented) LSP commands: `tinymist.exportXXX`/`previewXXX`, suitable for vscode or neovim clients, which allows client-side extension.
- (Not Implemented) External tools: Tools that update the lock file directly. For example, the tytanic could update all example documents once executed test commands. The official typst could also do this to tell whether a test case is compiled targeting HTML or PDF.

= Utilizing Compilation History

- Correct Entry Detection: When a user runs the compile or preview commands, Tinymist will save the compilation history to the lock file and the language server will identify the main file correctly.
- Dynamic Entry Switching: When a user runs another compile command, the newer command will have higher priorit, and Tinymist will "switch" the main file accordingly.
- Per-Document Flags: Some documents are compiled to HTML, while others are compiled to PDF. The users can specify more compile flags for each document.
- Session Persistence: Users can open the recently previewed file and continue editing it. More state such as the scroll position could be remembered in future.
- Sharing and VCS: The lock file can be shared with other users by tools like git, allowing them to compile the same project with the same settings.

= Project Route

*The depended files* of a single compilation will be stored in the cache directory after compilation. The order of the route entries is preserved to ensure that the most recent entry is used first. A single route entry is a triple, (Dependent Path, Project ID, Priority), where:
- "Dependent Path" is a depended file path.
- "Project ID" is the project id.
- "Priority" is a priority number.

The language server will load and compile route pairs and perform entry lookup based on the route entries.
+ Highest priority routes take precedence
+ Most recent updated commands prioritized automatically

= Storing Project Model Data

*Storage in file system*:
- `tinymist.lock` (TOML): When resolving a depended file, the nearest lock file will be used to determine the compilation arguments.
- Cache Directory:
  Contains project routes. Currently, we haven't implemented a way clean up or garbage collect the project routes. It is safe to remove all the project routes in the cache directory, as Tinymist will regenerate them when needed.

*Storage in memory*:
- The language server also maintains a compilation history and project routes in memory.
- We may enable in-memory compilation history by default in the future, which will allow Tinymist to resolve projects smarter.

= Stability Notice: `tinymist.lock`

We have been aware of backward compatibility issues, but the break change of the schema of `tinymist.lock` may corrupt the `tinymist.lock`. The schema is unstable and in beta stage. You have to remove the `tinymist.lock` file to recovery from errors, and optionally open an issue to discuss with us. To reliably keep compilation commands, please put `tinymist compile` commands in build system such as `Makefile` or `justfile`.
