# Change Log

All notable changes to the "tinymist" extension will be documented in this file.

Check [Keep a Changelog](http://keepachangelog.com/) for recommendations on how to structure this file.

The changelog lines unspecified with authors are all written by the @Myriad-Dreamin.

## v0.12.19 - [2025-02-03]

Nightly Release at [feat: generate declarative project lock file (#1133)](https://github.com/Myriad-Dreamin/tinymist/commit/bdfc1ed648f040b1c552d43f8ee7c9e9c882544e), using [ParaN3xus/typst tinymist-nightly-v0.12.19-rc2-content-hint](https://github.com/ParaN3xus/typst/tree/tinymist-nightly-v0.12.19-rc2-content-hint), a.k.a. [typst/typst Support first-line-indent for every paragraph (#5768)](https://github.com/typst/typst/commit/85d177897468165b93056947a80086b2f84d815d).

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.12.18...v0.12.19

## v0.12.18 - [2025-01-09]

We have added maintainers to GitHub since 2025-01-09:
- @SylvanFranklin has become a maintainer of the "Editor integration" and "Document Previewing" feature in https://github.com/Myriad-Dreamin/tinymist/pull/1091

After a super long time of development, we have finished an usable typst grammar for VS Code and GitHub. The grammar can now successfully parse all code, markup and math syntax of source files from [typst/packages (1200k LoCs)](https://github.com/typst/packages) and [typst/typst (17k LoCs)](https://github.com/typst/typst) without failure. A failure means the grammar produces any visible parse error but the official parser doesn't complain. However, it can still only parse a subset of typst syntax correctly:
- For example, all braces in `#if {}=={}{}{}` (without spaces) are identified as code braces.
- For example, It hasn't identified the ";" syntax in math calls.
But I believe it will not affect us much `:)`.

Most importantly, ideally GitHub will use the grammar to highlight typst code on GitHub in next season. It would be appreciated if people could check and test the grammar before GitHub's integration. the grammar and two ways to test it:

- The grammar: https://github.com/michidk/typst-grammar/blob/main/grammars/typst.tmLanguage.json
- Run Grammar's [snapshot tests](https://github.com/Myriad-Dreamin/tinymist/tree/main/syntaxes/textmate#testing) and GitHub's [integration tests](https://github.com/Myriad-Dreamin/tinymist/tree/main/syntaxes/textmate#github-integration).
- Install tinymist and check syntax highlight of [typst/packages](https://github.dev/typst/packages) in VS Code Web.

### Editor

* Building tinymist targeting web in https://github.com/Myriad-Dreamin/tinymist/pull/1102
* Bootstrapping lsp-free features in web in https://github.com/Myriad-Dreamin/tinymist/pull/1105

### Code Analysis

* Matching param names for completion in https://github.com/Myriad-Dreamin/tinymist/pull/1113

### Completion

* Completing parameters by capture information in https://github.com/Myriad-Dreamin/tinymist/pull/1114
* (Fix) Corrected order to insert definitions in scope in https://github.com/Myriad-Dreamin/tinymist/pull/1116

### Hover

* Rearranged hover providers in https://github.com/Myriad-Dreamin/tinymist/pull/1108
  * Definitions, (sampled) possible values, periscope, docs, actions are provided in order at the same time.

### Syntax Highlighting

* Adding experimental math syntax highlighting in https://github.com/Myriad-Dreamin/tinymist/pull/1096, https://github.com/Myriad-Dreamin/tinymist/pull/1106, https://github.com/Myriad-Dreamin/tinymist/pull/1112, https://github.com/Myriad-Dreamin/tinymist/pull/1117, https://github.com/Myriad-Dreamin/tinymist/pull/1123, and https://github.com/Myriad-Dreamin/tinymist/pull/1124
* Enabled experimental math syntax highlighting in https://github.com/Myriad-Dreamin/tinymist/pull/1107
* Parsing name identifier of parameters or arguments in https://github.com/Myriad-Dreamin/tinymist/pull/1118
* Changing names of string, constant, and keyword scopes in https://github.com/Myriad-Dreamin/tinymist/pull/1119
* Matching special identifiers in calls in https://github.com/Myriad-Dreamin/tinymist/pull/1125
* Added scripts to test syntax highlight in https://github.com/Myriad-Dreamin/tinymist/pull/1121
* Added more termination rules about FIRST tokens in https://github.com/Myriad-Dreamin/tinymist/pull/1122 and https://github.com/Myriad-Dreamin/tinymist/pull/1129
* Parsing arrow functions like binary expr in https://github.com/Myriad-Dreamin/tinymist/pull/1128
* Conditionally satisfying PCRE regex features in https://github.com/Myriad-Dreamin/tinymist/pull/1126 and https://github.com/Myriad-Dreamin/tinymist/pull/1130
* Documenting textmate grammar in https://github.com/Myriad-Dreamin/tinymist/pull/1131

### Misc

* Changed name in package.json files by @Freed-Wu and @Myriad-Dreamin in https://github.com/Myriad-Dreamin/tinymist/pull/1097 and https://github.com/Myriad-Dreamin/tinymist/pull/1102
* Ignoring vscode workspace configuration in https://github.com/Myriad-Dreamin/tinymist/pull/1120

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.12.16...v0.12.18

## v0.12.16 - [2025-01-02]

We are going to add maintainers to GitHub since 2025-01-07 (in 7 days):
- @SylvanFranklin want to maintain the "Editor integration" and "Document Previewing" feature in https://github.com/Myriad-Dreamin/tinymist/pull/1091

*Please reply in PRs or DM @Myriad-Dreamin if you have any concerns about adding the maintainer to list.*

### Completion

* (Fix) Completing body of let/closure in markup mode in https://github.com/Myriad-Dreamin/tinymist/pull/1072
* (Fix) Completing raw language again in https://github.com/Myriad-Dreamin/tinymist/pull/1073
* (Fix) Completing hash expression in math mode in https://github.com/Myriad-Dreamin/tinymist/pull/1071
* Completing context expression in code mode in https://github.com/Myriad-Dreamin/tinymist/pull/1070
* Using more efficient completion data structure in https://github.com/Myriad-Dreamin/tinymist/pull/1079

### Rename

* (Fix) Checking to avoid affecting non-related paths when renaming files by @Eric-Song-Nop in https://github.com/Myriad-Dreamin/tinymist/pull/1080

### Folding Range

* Folding continuous comments by @Eric-Song-Nop in https://github.com/Myriad-Dreamin/tinymist/pull/1043

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.12.14...v0.12.16
