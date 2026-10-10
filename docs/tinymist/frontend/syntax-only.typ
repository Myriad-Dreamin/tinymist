#import "mod.typ": *
When working under power-saving mode or with resource-consumed projects, typst compilations costs too much CPU and memory resources. You can configure the extension to run in syntax only mode, i.e. only performing elementary tasks, like syntax checking, syntax-only code analysis and formatting by setting `tinymist.syntaxOnly` (the VS Code extension setting) or the `syntaxOnly` server option (used by other editors’ language server configurations) to `enable` or `onPowerSaving`.

For more information about power-saving mode, see #cross-link("/feature/syntax-only-mode.typ")[Syntax-Only Mode].
