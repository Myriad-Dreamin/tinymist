#import "mod.typ": *
#import "shared.typ": is-vscode
#import "../config/shared.typ": show-switch

#show: book-page.with(title: [Neovim Software Specification])

#is-vscode.update(false)
#include "shared.typ"

= Typst-Specific LSP Configurations

#show-switch.update(false)
#include "../config/shared.typ"
