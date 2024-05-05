# Change Log

All notable changes to the "tinymist" extension will be documented in this file.

Check [Keep a Changelog](http://keepachangelog.com/) for recommendations on how to structure this file.

## v0.11.7 - [2024-05-05]

### Editor

* Improved icons in https://github.com/Myriad-Dreamin/tinymist/pull/242
* Conditionally opening activity icon when lang id is typst by @Enter-tainer in https://github.com/Myriad-Dreamin/tinymist/pull/222
* (Fix) Symbol view issues in https://github.com/Myriad-Dreamin/tinymist/pull/224
* Disable inlay hints by default in https://github.com/Myriad-Dreamin/tinymist/pull/225

### Completion

* Triggering parameter hints instead of suggest on pos args in https://github.com/Myriad-Dreamin/tinymist/pull/243
* Showing label descriptions for labels in https://github.com/Myriad-Dreamin/tinymist/pull/228 and https://github.com/Myriad-Dreamin/tinymist/pull/237
* Showing graphic label descriptions for symbols in https://github.com/Myriad-Dreamin/tinymist/pull/227 and https://github.com/Myriad-Dreamin/tinymist/pull/237
* feat: label descriptions according to types in https://github.com/Myriad-Dreamin/tinymist/pull/237
* Filtering completions by module import in https://github.com/Myriad-Dreamin/tinymist/pull/234
* Filtering completions by surrounding syntax for elements/selectors in https://github.com/Myriad-Dreamin/tinymist/pull/236

### Code Action (New)

* feat: provide code action to rewrite headings in https://github.com/Myriad-Dreamin/tinymist/pull/240

### Definition

* Finding definition of label references in https://github.com/Myriad-Dreamin/tinymist/pull/235

### Hover

* Handled/Added link in the hover documentation in https://github.com/Myriad-Dreamin/tinymist/pull/239

### Signature Help

* Reimplemented signature help with static analyses in https://github.com/Myriad-Dreamin/tinymist/pull/241

### Misc

* Added template for feature request in https://github.com/Myriad-Dreamin/tinymist/pull/238
* Improved Dynamic analysis on import from dynamic expressions in https://github.com/Myriad-Dreamin/tinymist/pull/233
* Performing Type check across modules in https://github.com/Myriad-Dreamin/tinymist/pull/232
* Bumped to typstyle v0.11.17 by @Enter-tainer in https://github.com/Myriad-Dreamin/tinymist/pull/223

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.11.6...v0.11.7

## v0.11.6 - [2024-04-27]

### Editor

* Added more auto closing pairs, surrounding pairs, and characters that could make auto closing before in https://github.com/Myriad-Dreamin/tinymist/pull/209
* Hiding Status bar until the recent focus file is closed in https://github.com/Myriad-Dreamin/tinymist/pull/212

### Compiler

* (Fix) Removed a stupid debugging which may cause panic in https://github.com/Myriad-Dreamin/tinymist/pull/215

### Commands/Tools

* Completed symbol view in https://github.com/Myriad-Dreamin/tinymist/pull/218
  * Not all symbols are categorized yet. If not, they are put into the "Misc" category.
  * It is now showing in the activity bar (sidebar). Feel free to report any issues or suggestions for improvement.

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.11.5...v0.11.6

## v0.11.5 - [2024-04-20]

### Completion

* Fixed wrong check of param completion position at comma in https://github.com/Myriad-Dreamin/tinymist/pull/205
* Completing text.lang/region in https://github.com/Myriad-Dreamin/tinymist/pull/199
* Completing array/tuple literals in https://github.com/Myriad-Dreamin/tinymist/pull/201
  * New array types completed: columns/ros/gutter/column-gutter/row-gutter/size/dash on various functions
* Completing function arguments on signatures inferred by type checking in https://github.com/Myriad-Dreamin/tinymist/pull/203
* Completing function arguments of func.where and func.with by its method target (this) in https://github.com/Myriad-Dreamin/tinymist/pull/204
* Completing functions with where/with snippets in https://github.com/Myriad-Dreamin/tinymist/pull/206

### Inlay Hint

* Checking variadic/content arguments rules of inlay hints correctly in https://github.com/Myriad-Dreamin/tinymist/pull/202

### Syntax/Semantic Highlighting

* (Fix) Corrected parsing on reference names of which trailing dots or colons cannot be followed by space or EOF in https://github.com/Myriad-Dreamin/tinymist/pull/195
* (Fix) Identifying string literals in math mode in https://github.com/Myriad-Dreamin/tinymist/pull/196

### Misc

* Bumped to typstyle v0.11.14 by @Enter-tainer in https://github.com/Myriad-Dreamin/tinymist/pull/200
* Preferring less uses of `analzer_expr` during definition analysis in https://github.com/Myriad-Dreamin/tinymist/pull/192

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.11.4...v0.11.5

## v0.11.4 - [2024-04-14]

This version is published with mostly internal optimizations.

### Editor

* (Change) Renamed trace feature to profile feature in https://github.com/Myriad-Dreamin/tinymist/pull/185

### Compiler

* (Fix) Set entry state on changing entry in https://github.com/Myriad-Dreamin/tinymist/pull/180
  * will cause incorrect label completion.

### Completion

* Autocompleting with power of type inference in https://github.com/Myriad-Dreamin/tinymist/pull/183, https://github.com/Myriad-Dreamin/tinymist/pull/186, and https://github.com/Myriad-Dreamin/tinymist/pull/189
  * See full list at https://github.com/Myriad-Dreamin/tinymist/blob/878a4146468b2a0e7a4435d7d0636df4f2133907/crates/tinymist-query/src/analysis/ty/builtin.rs
* (Fix) slicing at an offset that is not char boundary in https://github.com/Myriad-Dreamin/tinymist/pull/188

### Formatting

* Bumped typstyle to v0.11.13 by @Enter-tainer in https://github.com/Myriad-Dreamin/tinymist/pull/181

### Syntax/Semantic Highlighting

* Provided better grammar on incomplete heading in https://github.com/Myriad-Dreamin/tinymist/pull/187

### Misc

* (Fix) Improved release profile & fix typos by @QuarticCat in https://github.com/Myriad-Dreamin/tinymist/pull/177

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.11.3...v0.11.4

## v0.11.3 - [2024-04-06]

### Editor

* (Fix) Skipped tabs that have no URIs for reopening pdf in https://github.com/Myriad-Dreamin/tinymist/pull/147

### Compiler

* ~~Evicting cache more frequently in https://github.com/Myriad-Dreamin/tinymist/pull/161~~
  * Reverted in https://github.com/Myriad-Dreamin/tinymist/pull/173.
* (Fix) Collecting warning diagnostics correctly in https://github.com/Myriad-Dreamin/tinymist/pull/169

### Commands/Tools

* Introduced summary page in https://github.com/Myriad-Dreamin/tinymist/pull/137, https://github.com/Myriad-Dreamin/tinymist/pull/154, https://github.com/Myriad-Dreamin/tinymist/pull/162, and https://github.com/Myriad-Dreamin/tinymist/pull/168
* Introduced symbol picker in https://github.com/Myriad-Dreamin/tinymist/pull/155
* Introduced periscope mode previewing in https://github.com/Myriad-Dreamin/tinymist/pull/164
* Introduced status bar for showing words count, also for compiling status in https://github.com/Myriad-Dreamin/tinymist/pull/158
* Supported tracing execution in current document in https://github.com/Myriad-Dreamin/tinymist/pull/166

### Color Provider (New)

* Added basic color providers in https://github.com/Myriad-Dreamin/tinymist/pull/171

### Completion

* (Fix) Performed correct dynamic analysis on imports in https://github.com/Myriad-Dreamin/tinymist/pull/143
* (Fix) Correctly shadowed items for completion in https://github.com/Myriad-Dreamin/tinymist/pull/145
* (Fix) Completing parameters in scope in https://github.com/Myriad-Dreamin/tinymist/pull/146
* Completing parameters on user functions in https://github.com/Myriad-Dreamin/tinymist/pull/148
* Completing parameter values on user functions in https://github.com/Myriad-Dreamin/tinymist/pull/149
* Triggering autocompletion again after completing a function in https://github.com/Myriad-Dreamin/tinymist/pull/150
* Recovered module completion in https://github.com/Myriad-Dreamin/tinymist/pull/151

### Syntax/Semantic Highlighting

* (Fix) Improved grammar on incomplete AST in https://github.com/Myriad-Dreamin/tinymist/pull/140
* (Fix) Correctly parsing label and reference markup in https://github.com/Myriad-Dreamin/tinymist/pull/167

### Definition

* Supported go to paths to `#include` statement in https://github.com/Myriad-Dreamin/tinymist/pull/156

### Formatting

* Bumped to typstyle v0.11.11 by @Enter-tainer in https://github.com/Myriad-Dreamin/tinymist/pull/163
* Added common print width configuration for formatters in https://github.com/Myriad-Dreamin/tinymist/pull/170

### Hover (Tooltip)

* Joining array of hover contents by divider for neovim clients in https://github.com/Myriad-Dreamin/tinymist/pull/157

### Internal Optimization

* Analyzing lexical hierarchy on for loops in https://github.com/Myriad-Dreamin/tinymist/pull/142
  * depended by autocompletion/definition/references/rename APIs.

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.11.2...v0.11.3

## v0.11.2 - [2024-03-30]

### Editor

* (Fix) Passed correct arguments to editor tools in https://github.com/Myriad-Dreamin/tinymist/pull/111
* (Fix) exposed pin/unpin commands for vscode in https://github.com/Myriad-Dreamin/tinymist/pull/121

### Compiler

* (Fix) Converting out of bounds offsets again in https://github.com/Myriad-Dreamin/tinymist/pull/115
* Supported entry configuration in https://github.com/Myriad-Dreamin/tinymist/pull/122
* Supported untitled url scheme for unsaved text buffer in https://github.com/Myriad-Dreamin/tinymist/pull/120 and https://github.com/Myriad-Dreamin/tinymist/pull/130

### Commands/Tools

* Allowed tracing typst programs in subprocess in https://github.com/Myriad-Dreamin/tinymist/pull/112
  * This is part of backend for tracing tool, and we may finish a tracing tool in next week.

### Formatting

* Supported formatters in https://github.com/Myriad-Dreamin/tinymist/pull/113
  * Use `"formatterMode": "typstyle"` for `typstyle 0.11.7`
  * Use `"formatterMode": "typstfmt"` for `typstfmt 0.2.9`
* feat: minimal diff algorithm for source formatting in https://github.com/Myriad-Dreamin/tinymist/pull/123

### Completion

* Fixed wrong completion kind in https://github.com/Myriad-Dreamin/tinymist/pull/124 and https://github.com/Myriad-Dreamin/tinymist/pull/127
* Supported import path completion in https://github.com/Myriad-Dreamin/tinymist/pull/134
* Not completing on definition itself anymore in https://github.com/Myriad-Dreamin/tinymist/pull/135

### Syntax/Semantic Highlighting

* (Fix) Corrected identifier/keyword boundaries in https://github.com/Myriad-Dreamin/tinymist/pull/128
* Improved punctuation and keyword token kinds in https://github.com/Myriad-Dreamin/tinymist/pull/133

### Hover (Tooltip)

* fix: parse docstring dedents correctly in https://github.com/Myriad-Dreamin/tinymist/pull/132

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.11.1...v0.11.2

## v0.11.1 - [2024-03-26]

### Editor

* Integrated neovim support in https://github.com/Myriad-Dreamin/tinymist/pull/91
* docs: mention how to work with multiple-file projects in https://github.com/Myriad-Dreamin/tinymist/pull/108
* feat: add minimal helix support in https://github.com/Myriad-Dreamin/tinymist/pull/107

### Compiler

* (Fix) Always uses latest compiled document for lsp functions in https://github.com/Myriad-Dreamin/tinymist/pull/68
* (Fix) Converts EOF position correctly in https://github.com/Myriad-Dreamin/tinymist/pull/92
* Allowed running server on rootless files and loading font once in https://github.com/Myriad-Dreamin/tinymist/pull/94
* Uses positive system font config in https://github.com/Myriad-Dreamin/tinymist/pull/93 and https://github.com/Myriad-Dreamin/tinymist/pull/97

### Syntax/Semantic Highlighting

* Provided correct semantic highlighting in https://github.com/Myriad-Dreamin/tinymist/pull/71
* Provided correct syntax highlighting in https://github.com/Myriad-Dreamin/tinymist/pull/77, https://github.com/Myriad-Dreamin/tinymist/pull/80, https://github.com/Myriad-Dreamin/tinymist/pull/85, and https://github.com/Myriad-Dreamin/tinymist/pull/109
* Colorizes contextual bracket according to textmate scopes in https://github.com/Myriad-Dreamin/tinymist/pull/81

### Commands/Tools

* Fixed two bugs during initializing template in https://github.com/Myriad-Dreamin/tinymist/pull/65
* Added svg and png export in code lens context in https://github.com/Myriad-Dreamin/tinymist/pull/101
* Added tracing frontend in https://github.com/Myriad-Dreamin/tinymist/pull/98
  * The frontend is implemented but there is trouble with the backend.

### Hover (Tooltip)

* Provided hover tooltip on user functions in https://github.com/Myriad-Dreamin/tinymist/pull/76
* Parses comments for hover tooltip in https://github.com/Myriad-Dreamin/tinymist/pull/78 and https://github.com/Myriad-Dreamin/tinymist/pull/105

### Misc

* Provided dhat instrumenting feature for heap usage analysis in https://github.com/Myriad-Dreamin/tinymist/pull/64
* Disabled lto in https://github.com/Myriad-Dreamin/tinymist/pull/84

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.11.0...v0.11.1

## v0.11.0 - [2024-03-17]

### Commands/Tools

* Fixed [Template gallery index.html is not included in packaging](https://github.com/Myriad-Dreamin/tinymist/issues/59) in https://github.com/Myriad-Dreamin/tinymist/pull/60

### Commands/Tools (New)

* Added favorite function in template gallery in https://github.com/Myriad-Dreamin/tinymist/pull/61
  * favorite or unfavorite by clicking a button.
  * filter list by favorite state.
  * get persist favorite state.
  * run `initTemplate` command with favorite state.
* Initializing template in place is allowed in https://github.com/Myriad-Dreamin/tinymist/pull/62
  * place the content of the template entry at the current cursor position.

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.10.3...v0.11.0

## v0.10.3 - [2024-03-16]

### Commands/Tools (New)

* support rest code lens in https://github.com/Myriad-Dreamin/tinymist/pull/45
  * Preview
  * Preview in ..
    * `doc` or `slide` mode
    * `tab` or `browser` target
  * Export as ..
    * PDF format
* add init template command in https://github.com/Myriad-Dreamin/tinymist/pull/50
* add template gallery as template picker in https://github.com/Myriad-Dreamin/tinymist/pull/52

### References (New)

* support find/goto syntactic references in https://github.com/Myriad-Dreamin/tinymist/pull/34 and https://github.com/Myriad-Dreamin/tinymist/pull/42

### Autocompletion

* upgrade compiler for autocompleting package in https://github.com/Myriad-Dreamin/tinymist/pull/30

### Definition

* dev: reimplements definition analysis in https://github.com/Myriad-Dreamin/tinymist/pull/43

### Inlay Hint

* implement inlay hint configuration in https://github.com/Myriad-Dreamin/tinymist/pull/37
* disable inlay hints on one line content blocks in https://github.com/Myriad-Dreamin/tinymist/pull/48
* dev: change position of inlay hint params in https://github.com/Myriad-Dreamin/tinymist/pull/51

### Misc

* supports vscode variables in configurations, more testing, and validation in https://github.com/Myriad-Dreamin/tinymist/pull/53
  * You can set root/server/font path(s) with vscode variables. The variables are listed in https://www.npmjs.com/package/vscode-variables.

### Internal Optimization

* deferred root resolution in https://github.com/Myriad-Dreamin/tinymist/pull/32
* allow fuzzy selection to deref targets in https://github.com/Myriad-Dreamin/tinymist/pull/46
* implements def-use analysis in https://github.com/Myriad-Dreamin/tinymist/pull/17, https://github.com/Myriad-Dreamin/tinymist/pull/19, https://github.com/Myriad-Dreamin/tinymist/pull/25, and https://github.com/Myriad-Dreamin/tinymist/pull/26

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.10.2...v0.10.3

## v0.10.2 - [2024-03-12]

* use implicit autocomplete in https://github.com/Myriad-Dreamin/tinymist/pull/3
* add the new context keyword in https://github.com/Myriad-Dreamin/tinymist/pull/6
* correctly drop sender after the server shutting down in https://github.com/Myriad-Dreamin/tinymist/pull/7
* support more foldable AST nodes in https://github.com/Myriad-Dreamin/tinymist/pull/11

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.10.1...v0.10.2

## v0.10.1 - [2024-03-11]

Initial release corresponding to Typst v0.11.0-rc1.
