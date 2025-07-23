#import "mod.typ": *
#import "shared.typ": is-vscode
#import "../config/shared.typ": show-switch

#show: book-page.with(title: [VS Code Extension Specification])

#is-vscode.update(true)
#include "shared.typ"

= Typst-Specific VS Code Configurations

#show-switch.update(false)
#include "../config/shared.typ"
