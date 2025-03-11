#import "mod.typ": *

#show: book-page.with(title: [Code Completion])

== Using LSP-Based Completion

LSP will serve completion if you enter _trigger characters_ in the editor. Currently, the trigger characters are:
+ any valid identifier character, like ```js 'a'``` or ```js 'Z'```.
+ ```js '#'```, ```js '('```, ```js '<'```, ```js '.'```, ```js ':'```, ```js '/'```, ```js '"'```, ```js '@'```, which is configured by LSP server.

#pro-tip[
  === VSCode:
  Besides, you can trigger the completion manually by pressing ```js Ctrl+Space``` in the editor.

  If ```js Ctrl+Space``` doesn't work, please check your IME settings or keybindings.
]

When an item is selected, it will be committed if some character is typed.
1. press ```js Esc``` to avoid commit.
1. press ```js Enter``` to commit one.
2. press ```js '.'``` to commit one for those that can interact with the dot operator.
3. press ```js ';'``` to commit one in code mode.
4. press ```js ','``` to commit one in list.

=== Label Completion

The LSP will keep watching and compiling your documents to get available labels for completion. Thus, if it takes a long time to compile your document, there will be an expected delay after each editing labels in document.

A frequently asked question is how to completing labels in sub files when writing in a multiple-file project. By default, you will not get labels from other files, e.g. bibiliography configured in other files. This is because the "main file" will be tracked when your are switching the focused files. Hence, the solution is to set up the main file correctly for the multi-file project.

#pro-tip[
  === VSCode:

  See #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/vscode#working-with-multiple-file-projects")[VS Code: Working with Multiple File Projects].
]

#pro-tip[
  === Neovim:

  See #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/neovim#working-with-multiple-file-projects")[Heovim: Working with Multiple File Projects].
]

#pro-tip[
  === Helix:

  See #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/helix#working-with-multiple-file-projects")[Helix: Working with Multiple File Projects].
]

== Using Snippet-Based Completion

#pro-tip[
  === VSCode:

  We suggest to use snippet extensions powered by TextMate Scopes. For example, #link("https://github.com/OrangeX4/OrangeX4-HyperSnips")[HyperSnips] provides context-sensitive snippet completion.
]
