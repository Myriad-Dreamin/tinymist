#import "mod.typ": *

#show: book-page.with(title: [Project Model])

This section documents experimental project management features. Implementation may change in future releases.

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

= Compilation History

The compilation history is a set of records. Each record contains the following information about compilation:
- *Input Args*: Main file path, fonts, features (e.g., HTML/Paged as export target).
- *Export Args*: Output path, PDF standard to use.
- *Dependencies*: Source files, assets.

The source of compilation history:
1. CLI commands: `tinymist compile/preview --save-lock`, suitable for all the editor clients.
2. LSP commands: `tinymist.exportXXX`/`previewXXX`, suitable for vscode or neovim clients, which allows client-side extension.
- External tools: Tools that update the lock file directly. For example, the tytanic could update all example documents once executed test commands. The official typst could also do this to tell whether a test case is compiled targeting HTML or PDF.

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

Currently, the scheme of `tinymist.lock` is unstable and may change in the future. To reliably keep compilation commands, please put `tinymist compile` commands in build system such as `Makefile` or `justfile`.
