#import "mod.typ": *
When working under power-saving mode or with resource-consumed projects, typst compilations costs too much CPU and memory resources. You can configure the extension to run in syntax only mode, i.e. stopping full document compilation and semantic analysis, while elementary tasks like syntax checking, syntax-level analysis (which keeps features like hover and definition working), and formatting still run, by setting the `tinymist.syntaxOnly` to `enable` or `onPowerSaving` in the configuration.

For more information about power-saving mode, see #cross-link("/feature/syntax-only-mode.typ")[Syntax-Only Mode].
