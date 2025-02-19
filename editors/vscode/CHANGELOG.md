# Change Log

All notable changes to the "tinymist" extension will be documented in this file.

Check [Keep a Changelog](http://keepachangelog.com/) for recommendations on how to structure this file.

The changelog lines unspecified with authors are all written by the @Myriad-Dreamin.

## v0.12.20 - [2025-02-21]

We massively changed the internal world implementation. This unblocks many new features:
- It computes dependencies of compilation accurately. It was not correct because compilations and analyzers runs on a same world at the same time.
- It only compiles documents when file changes really affect the compilation, because we now tracks dependencies correctly.
- It now adds new project model with a `tinymist.lock` to help manage documents and their dependencies on large multiple-files projects. This is still experimental and disabled by default.
- The `tinymist.lock` along with the browsing preview is expected to greatly help people work on large and complex projects using any of their faviorite editors.

* build: update `cc` version (#1162) in https://github.com/Myriad-Dreamin/tinymist/pull/1258
* build: downgrade `tempfile` to 3.15.0 in https://github.com/Myriad-Dreamin/tinymist/pull/1259
* build: upgrade typstyle to v0.12.15 by @Eric-Song-Nop in https://github.com/Myriad-Dreamin/tinymist/pull/1260 and https://github.com/Myriad-Dreamin/tinymist/pull/1324

### CLI

* feat: add CLI compile command and bench script in https://github.com/Myriad-Dreamin/tinymist/pull/1193

### Compiler

* dev: move package to reflexo_world part in https://github.com/Myriad-Dreamin/tinymist/pull/1177
* feat: move world implementation in https://github.com/Myriad-Dreamin/tinymist/pull/1183, https://github.com/Myriad-Dreamin/tinymist/pull/1185, https://github.com/Myriad-Dreamin/tinymist/pull/1186, https://github.com/Myriad-Dreamin/tinymist/pull/1187
* perf: reduce size of the watch entry in https://github.com/Myriad-Dreamin/tinymist/pull/1190
* perf: remove meta watch in https://github.com/Myriad-Dreamin/tinymist/pull/1191
* feat: track fine-grained revisions of `font`, `registry`, `entry`, and `vfs` in https://github.com/Myriad-Dreamin/tinymist/pull/1192
* feat: trigger project compilations on main thread in https://github.com/Myriad-Dreamin/tinymist/pull/1197
* feat: detect compilation-related vfs changes in https://github.com/Myriad-Dreamin/tinymist/pull/1199
* fix: try getting font index which is hit by comemo in https://github.com/Myriad-Dreamin/tinymist/pull/1213
* perf: scatter-gather the editor diagnostics in https://github.com/Myriad-Dreamin/tinymist/pull/1246
* fix: invalidate and increment revision in vfs correctly (#1292) in https://github.com/Myriad-Dreamin/tinymist/pull/1329
* fix: emit latest status and artifact with correct signals (#1294) in https://github.com/Myriad-Dreamin/tinymist/pull/1330
* fix: the path to join is shadowed by a local variable (#1322) in https://github.com/Myriad-Dreamin/tinymist/pull/1335
* fix: don't remove path mapping when invalidating vfs cache (#1316) in https://github.com/Myriad-Dreamin/tinymist/pull/1333

### Editor

* feat: show name of the compiling file in the status bar in https://github.com/Myriad-Dreamin/tinymist/pull/1147
* feat: support convert to typst table from xlsx file by @hongjr03 in https://github.com/Myriad-Dreamin/tinymist/pull/1100
* fix: update xlsx-parser package version to 0.2.3 by @hongjr03 in https://github.com/Myriad-Dreamin/tinymist/pull/1166
* feat: support drag-and-drop feature for .ods format by @hongjr03 in https://github.com/Myriad-Dreamin/tinymist/pull/1217
* feat: add more known image extensions to the drop provider in https://github.com/Myriad-Dreamin/tinymist/pull/1308
* refactor: rename source file name of the drop feature in https://github.com/Myriad-Dreamin/tinymist/pull/1309
* feat: add support to paste image into typst documents in https://github.com/Myriad-Dreamin/tinymist/pull/1306
* feat: cancel codelens if the any picker is cancelled in https://github.com/Myriad-Dreamin/tinymist/pull/1314

### Code Analysis

* feat: add `depended_{paths,{source_,}files}` methods in https://github.com/Myriad-Dreamin/tinymist/pull/1150
* feat: prefer to select the previous token when cursor is before a marker in https://github.com/Myriad-Dreamin/tinymist/pull/1175
* fix: capture docs before check init in https://github.com/Myriad-Dreamin/tinymist/pull/1195
* fix: consider interpret mode when classifying dot accesses in https://github.com/Myriad-Dreamin/tinymist/pull/1302
* feat: support more path types and add path parameters (#1312) in https://github.com/Myriad-Dreamin/tinymist/pull/1331

### Label View

* fix(vscode): make label view work when there's exactly one label by @tmistele in https://github.com/Myriad-Dreamin/tinymist/pull/1158

### Crityp (New)

* feat: micro benchmark support in https://github.com/Myriad-Dreamin/tinymist/pull/1160

### Typlite

* feat: evaluate table and grid by @hongjr03 in https://github.com/Myriad-Dreamin/tinymist/pull/1300
* feat: embed Markdown codes by @hongjr03 in https://github.com/Myriad-Dreamin/tinymist/pull/1296
* feat(typlite): render context block contextually by @hongjr03 in https://github.com/Myriad-Dreamin/tinymist/pull/1305
* fix(typlite): correct the wrong path by @hongjr03 in https://github.com/Myriad-Dreamin/tinymist/pull/1323

### Preview

* feat: Rescaling with Ctrl+=/- in browser (in addition to ctrl+wheel) by @tmistele in https://github.com/Myriad-Dreamin/tinymist/pull/1110
* fix: Prevent malicious websites from connecting to http / websocket server by @tmistele and @Myriad-Dreamin in https://github.com/Myriad-Dreamin/tinymist/pull/1157
* fix: respect that the port of the `expected_origin` can be zero (#1295) in https://github.com/Myriad-Dreamin/tinymist/pull/1337
* feat: browsing preview in https://github.com/Myriad-Dreamin/tinymist/pull/1234

### Codelens

* feat: move less used codelens into a single "more" codelens in https://github.com/Myriad-Dreamin/tinymist/pull/1315

### Wasm

* feat: build tinymist-world on web in https://github.com/Myriad-Dreamin/tinymist/pull/1184
* feat: adapts build meta for wasm target in https://github.com/Myriad-Dreamin/tinymist/pull/1243

### tinymist.lock

* feat: copy flock implementation from cargo in https://github.com/Myriad-Dreamin/tinymist/pull/1140
* Generating and updating declarative project lock file in https://github.com/Myriad-Dreamin/tinymist/pull/1133, https://github.com/Myriad-Dreamin/tinymist/pull/1149, https://github.com/Myriad-Dreamin/tinymist/pull/1151, https://github.com/Myriad-Dreamin/tinymist/pull/1152, https://github.com/Myriad-Dreamin/tinymist/pull/1153, https://github.com/Myriad-Dreamin/tinymist/pull/1154
* feat: model and document project tasks in https://github.com/Myriad-Dreamin/tinymist/pull/1202
* feat: associate lock file with toml language in https://github.com/Myriad-Dreamin/tinymist/pull/1143
* feat: initiate `lockDatabase` project resolution in https://github.com/Myriad-Dreamin/tinymist/pull/1201
* feat: resolve projects by `lockDatabase` in https://github.com/Myriad-Dreamin/tinymist/pull/1142
* feat: execute export and query on the task model in https://github.com/Myriad-Dreamin/tinymist/pull/1214
* feat: CLI compile documents with lock updates in https://github.com/Myriad-Dreamin/tinymist/pull/1218
* feat: CLI generate shell build script in https://github.com/Myriad-Dreamin/tinymist/pull/1219

### Misc

* docs: revise neovim's install section by @SylvanFranklin in https://github.com/Myriad-Dreamin/tinymist/pull/1090
* docs: add release instruction by @ParaN3xus and @Myriad-Dreamin in https://github.com/Myriad-Dreamin/tinymist/pull/1163, https://github.com/Myriad-Dreamin/tinymist/pull/1169, https://github.com/Myriad-Dreamin/tinymist/pull/1173, and https://github.com/Myriad-Dreamin/tinymist/pull/1212
* docs: documenting `sync-lsp` crate in https://github.com/Myriad-Dreamin/tinymist/pull/1155
* fix(ci): use deploy-pages v4 in https://github.com/Myriad-Dreamin/tinymist/pull/1249
* fix(ci): use upload-pages-artifact and configure-pages in https://github.com/Myriad-Dreamin/tinymist/pull/1251
* docs: documenting Myriad-Dreamin's workspace setting in https://github.com/Myriad-Dreamin/tinymist/pull/1264
* docs: fix typo by @YDX-2147483647 in https://github.com/Myriad-Dreamin/tinymist/pull/1276
* feat: add release crates action in https://github.com/Myriad-Dreamin/tinymist/pull/1298

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.12.18...v0.12.20

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
