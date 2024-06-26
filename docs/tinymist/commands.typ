#import "mod.typ": *

#show: book-page.with(title: "Tinymist Command System")

The extra features are exposed via LSP's #link("https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#workspace_executeCommand")[`workspace/executeCommand`] request, forming a command system. The commands in the system share a name convention.

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
