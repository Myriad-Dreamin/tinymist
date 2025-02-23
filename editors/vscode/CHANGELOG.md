# Change Log

All notable changes to the "tinymist" extension will be documented in this file.

Check [Keep a Changelog](http://keepachangelog.com/) for recommendations on how to structure this file.

The changelog lines unspecified with authors are all written by the @Myriad-Dreamin.

## v0.12.22 - [2025-02-23]

### Compiler

* (Fix) Applying memory changes to dedicate instances in https://github.com/Myriad-Dreamin/tinymist/pull/1371
  * This fixes the issue that the second preview tab is updated.

### Preview

* (Fix) Handling compile events in standalone preview server in https://github.com/Myriad-Dreamin/tinymist/pull/1349
* (Fix) Loosing `origin` HTTP header checking of the preview server in https://github.com/Myriad-Dreamin/tinymist/pull/1353
* (Fix) Added console diagnostics printing back for `tinymist preview` in https://github.com/Myriad-Dreamin/tinymist/pull/1359
* (Fix) Fixed broken regular preview affected by the browsing preview feature in https://github.com/Myriad-Dreamin/tinymist/pull/1357 and https://github.com/Myriad-Dreamin/tinymist/pull/1358
* (Fix) Sharing preview handler among states in https://github.com/Myriad-Dreamin/tinymist/pull/1370
  * This fixes the issue that a user can't open multiple preview tabs at the same time.

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.12.20...v0.12.22

## v0.12.20 - [2025-02-21]

We massively changed the internal world implementation. This unblocks many new features:
- It computes dependencies of compilation accurately. It was not correct because compilations and analyzers runs on a same world at the same time.
- It only compiles documents when file changes really affect the compilation, because we now tracks dependencies correctly.
- It now adds new project model with a `tinymist.lock` to help manage documents and their dependencies on large multiple-files projects. This is still experimental and disabled by default.
- The `tinymist.lock` along with the browsing preview is expected to greatly help people work on large and complex projects using any of their faviorite editors.

For `tinymist.lock` feature, please check the [tinymist.projectResolution = "lockDatabase"](https://github.com/Myriad-Dreamin/tinymist/blob/main/editors/vscode/Configuration.md#tinymistprojectresolution). This is still experimental for multiple-files projects.

* Bumped `cc` to v1.2.11 (#1162) in https://github.com/Myriad-Dreamin/tinymist/pull/1258
* Downgraded `tempfile` to v3.15.0 in https://github.com/Myriad-Dreamin/tinymist/pull/1259
* Bumped typstyle to v0.12.15 by @Eric-Song-Nop in https://github.com/Myriad-Dreamin/tinymist/pull/1260 and https://github.com/Myriad-Dreamin/tinymist/pull/1324

### CLI

* Added CLI compile command in https://github.com/Myriad-Dreamin/tinymist/pull/1193 and https://github.com/Myriad-Dreamin/tinymist/pull/1218
  * The compile command is mainly used for compiling documents with updating lock file. Using:

  ```
  tinymist compile --save-lock tinymist.lock
  ```
  * This could also be used for comparing the coompile performance of `tinymist-cli` and `typst-cli`.
* Generating shell build script according to the lock file in https://github.com/Myriad-Dreamin/tinymist/pull/1219

### Compiler

* (Fix) Fixed a panic when getting font index which is hit by comemo in https://github.com/Myriad-Dreamin/tinymist/pull/1213
  * This could be true when the fonts are hot reloaded.
* (Fix) Emiting latest status and artifact with correct signals (#1294) in https://github.com/Myriad-Dreamin/tinymist/pull/1330
  * Because of this, the compile status bar was not updated correctly.
* (Perf) Detecting compilation-related vfs changes in https://github.com/Myriad-Dreamin/tinymist/pull/1199
* (Perf) Scatter-gathering the editor diagnostics in https://github.com/Myriad-Dreamin/tinymist/pull/1246
* Moved world implementation to tinymist in https://github.com/Myriad-Dreamin/tinymist/pull/1177, https://github.com/Myriad-Dreamin/tinymist/pull/1183, https://github.com/Myriad-Dreamin/tinymist/pull/1185, https://github.com/Myriad-Dreamin/tinymist/pull/1186, and https://github.com/Myriad-Dreamin/tinymist/pull/1187
* Reduced size of the watch entry in https://github.com/Myriad-Dreamin/tinymist/pull/1190 and https://github.com/Myriad-Dreamin/tinymist/pull/1191
* Tracking fine-grained revisions of `font`, `registry`, `entry`, and `vfs` in https://github.com/Myriad-Dreamin/tinymist/pull/1192
  * This prepares for better configuration hot reloading in future.
* Triggering project compilations on main thread in https://github.com/Myriad-Dreamin/tinymist/pull/1197
  * This helps apply more advanced compilation strategy with sync and mutable state on the main thread. For example, [Filtering out unreleated file changes](https://github.com/Myriad-Dreamin/tinymist/pull/1199) has been applied.
### Editor

* Showing name of the compiling file in the status bar in https://github.com/Myriad-Dreamin/tinymist/pull/1147
  * You can customize it by setting `tinymist.statusBarFormat` in the settings.

### Drop and Paste

* Added support to drag and drop `.xlsx` files by @hongjr03 in https://github.com/Myriad-Dreamin/tinymist/pull/1100 and https://github.com/Myriad-Dreamin/tinymist/pull/1166
* Added support to drag and drop `.ods` files by @hongjr03 in https://github.com/Myriad-Dreamin/tinymist/pull/1217
* Added more known image extensions to the drop provider in https://github.com/Myriad-Dreamin/tinymist/pull/1308
  * Added `.avif`, `.jpe`, `.psd`, `.tga`, `.tif`, and `.tiff`, which are copied from the markdown extension. 
* Added support to paste media files (images, audios, and videos) into typst documents in https://github.com/Myriad-Dreamin/tinymist/pull/1306
* Canceling codelens if any picker is cancelled in https://github.com/Myriad-Dreamin/tinymist/pull/1314

### Label View

* (Fix) Making label view work when there's exactly one label by @tmistele in https://github.com/Myriad-Dreamin/tinymist/pull/1158

### Typlite

* Evaluating table and grid by @hongjr03 in https://github.com/Myriad-Dreamin/tinymist/pull/1300
* Embedding Markdown codes by `typlite` raw block by @hongjr03 in https://github.com/Myriad-Dreamin/tinymist/pull/1296 and https://github.com/Myriad-Dreamin/tinymist/pull/1323
* Rendering context block contextually by @hongjr03 in https://github.com/Myriad-Dreamin/tinymist/pull/1305

### Preview

* (Fix) Logging error on channel closed instead of panicking in https://github.com/Myriad-Dreamin/tinymist/pull/1347
  * This may happen when the preview is broadcasting and the clients hasn't connected to the server.
* Rescaling with Ctrl+=/- in browser (in addition to ctrl+wheel) by @tmistele in https://github.com/Myriad-Dreamin/tinymist/pull/1110
* Prevented malicious websites from connecting to http / websocket server by @tmistele and @Myriad-Dreamin in https://github.com/Myriad-Dreamin/tinymist/pull/1157 and https://github.com/Myriad-Dreamin/tinymist/pull/1337
* Browsing preview in https://github.com/Myriad-Dreamin/tinymist/pull/1234

### Code Analysis

* (Fix) Capturing docs before check init in https://github.com/Myriad-Dreamin/tinymist/pull/1195
* (Fix) Considering interpret mode when classifying dot accesses in https://github.com/Myriad-Dreamin/tinymist/pull/1302
* Added `depended_{paths,{source_,}files}` methods in https://github.com/Myriad-Dreamin/tinymist/pull/1150
* Preferring to select the previous token when cursor is before a marker in https://github.com/Myriad-Dreamin/tinymist/pull/1175
* Support more path types and add path parameters (#1312) in https://github.com/Myriad-Dreamin/tinymist/pull/1331
  * Completes mutiple paths on `bibliography` and completes wasm files on `plugin`.

### Crityp (New)

* Added micro benchmark support in https://github.com/Myriad-Dreamin/tinymist/pull/1160
  * For example, the benchmark shows that `fib(20)` on rust (16us) is 40 times faster than that on typst (940us).
  * Check [crityp](https://github.com/Myriad-Dreamin/tinymist/blob/main/crates/crityp/README.md) for usage.

### Codelens

* Moved less used codelens into a single "more" codelens in https://github.com/Myriad-Dreamin/tinymist/pull/1315

### Wasm

* Building tinymist-world on web in https://github.com/Myriad-Dreamin/tinymist/pull/1184 and https://github.com/Myriad-Dreamin/tinymist/pull/1243

### tinymist.lock

* Copied flock implementation from cargo in https://github.com/Myriad-Dreamin/tinymist/pull/1140
* Generating and updating declarative project lock file in https://github.com/Myriad-Dreamin/tinymist/pull/1133, https://github.com/Myriad-Dreamin/tinymist/pull/1149, https://github.com/Myriad-Dreamin/tinymist/pull/1151, https://github.com/Myriad-Dreamin/tinymist/pull/1152, https://github.com/Myriad-Dreamin/tinymist/pull/1153, https://github.com/Myriad-Dreamin/tinymist/pull/1154
* Modeling project tasks in https://github.com/Myriad-Dreamin/tinymist/pull/1202
* Associating `tinymist.lock` with toml language in https://github.com/Myriad-Dreamin/tinymist/pull/1143
* Initiating `lockDatabase` project resolution in https://github.com/Myriad-Dreamin/tinymist/pull/1201
* Resolving projects by `lockDatabase` in https://github.com/Myriad-Dreamin/tinymist/pull/1142
* Executing export and query on the task model in https://github.com/Myriad-Dreamin/tinymist/pull/1214

### Misc

* Revised neovim's install section by @SylvanFranklin and @YDX-2147483647 in https://github.com/Myriad-Dreamin/tinymist/pull/1090 and https://github.com/Myriad-Dreamin/tinymist/pull/1276
* Added release instruction by @ParaN3xus and @Myriad-Dreamin in https://github.com/Myriad-Dreamin/tinymist/pull/1163, https://github.com/Myriad-Dreamin/tinymist/pull/1169, https://github.com/Myriad-Dreamin/tinymist/pull/1173, and https://github.com/Myriad-Dreamin/tinymist/pull/1212
* Documenting `sync-lsp` crate in https://github.com/Myriad-Dreamin/tinymist/pull/1155
* CI used newest deploy-pages, upload-pages-artifact, and configure-pages actions in https://github.com/Myriad-Dreamin/tinymist/pull/1249 and https://github.com/Myriad-Dreamin/tinymist/pull/1251
* Documenting Myriad-Dreamin's workspace setting in https://github.com/Myriad-Dreamin/tinymist/pull/1264
* CI Added release crates action in https://github.com/Myriad-Dreamin/tinymist/pull/1298
 * Published {tinymist-{derive,analysis,std,vfs,world,project},typlite,crityp} crates in https://github.com/Myriad-Dreamin/tinymist/pull/1310

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
