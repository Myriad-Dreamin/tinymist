#import "mod.typ": *

#show: book-page.with(title: [Editor Frontends])

Leveraging the interface of LSP, tinymist provides frontends to each editor, located in the #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors")[editor folders]. They are minimal, meaning that LSP should finish its main LSP features as many as possible without help of editor frontends. The editor frontends just enhances your code experience. For example, the vscode frontend takes responsibility on providing some nice editor tools. It is recommended to install these editors frontend for your editors.

Check the following chapters for uses:
- #cross-link("/frontend/vscode.typ")[VS Cod(e,ium)]
- #cross-link("/frontend/neovim.typ")[NeoVim]
- #cross-link("/frontend/emacs.typ")[Emacs]
- #cross-link("/frontend/sublime-text.typ")[Sublime Text]
- #cross-link("/frontend/helix.typ")[Helix]
- #cross-link("/frontend/zed.typ")[Zed]
