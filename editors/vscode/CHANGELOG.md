# Change Log

All notable changes to the "tinymist" extension will be documented in this file.

Check [Keep a Changelog](http://keepachangelog.com/) for recommendations on how to structure this file.

## v0.11.18 - [2024-08-05]

### Compiler

* Cherry picked concurrent id error in https://github.com/Myriad-Dreamin/tinymist/pull/472
  * This affects lsp since the server parallelized the requests.
* (Fix) Retrieving environments even if `typstExtraArgs` is unspecified in https://github.com/Myriad-Dreamin/tinymist/pull/482
  * For example, the env variable `SOURCE_DATE_EPOCH` is not used when `typstExtraArgs` is not specified.

### Commands/Tools

* Supported vscode tasks for exporting pdf, svg, and png in https://github.com/Myriad-Dreamin/tinymist/pull/488
* Supported vscode tasks for exporting html, md, and txt in https://github.com/Myriad-Dreamin/tinymist/pull/489
* Supported vscode tasks for exporting query and pdfpc in https://github.com/Myriad-Dreamin/tinymist/pull/490

### Preview

* Added normal-image option for `tinymist.preview.invertColor` feature by @SetsuikiHyoryu in https://github.com/Myriad-Dreamin/tinymist/pull/464 and https://github.com/Myriad-Dreamin/tinymist/pull/473
  * People may love inverted color for preview, but not for images. This feature helps them.
* Removed `typst-preview.showLog` and added `tinymist.showLog` in https://github.com/Myriad-Dreamin/tinymist/pull/476
* (Fix) Processing task id correctly when executing scroll command in https://github.com/Myriad-Dreamin/tinymist/pull/477

### Completion

* (Fix) Applying label instead of bib title name in `at` completion by @kririae in https://github.com/Myriad-Dreamin/tinymist/pull/485

### Syntax/Semantic Highlighting

* (Fix) Allowing hyphenate in url link in https://github.com/Myriad-Dreamin/tinymist/pull/481
  * It was not highlighted correctly.

### Misc

* Added documentation about installing nightly prebuilts in https://github.com/Myriad-Dreamin/tinymist/pull/480
* Improved contribution guide and added sections for syntaxes in https://github.com/Myriad-Dreamin/tinymist/pull/471

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.11.17...v0.11.18

## v0.11.17 - [2024-07-27]

### Editor

* Added a `$(file-pdf)` icon for `showPdf` to navigation bar in https://github.com/Myriad-Dreamin/tinymist/pull/462
  * It is a shorter way to export and open documents as PDF.
  * It now has a different icon from the `preview` command.
  * Note: This function is suitable to help perform your final checks to documents. For previewing, please uses `preview` command for better experience.
* Interned vscode-variable package in https://github.com/Myriad-Dreamin/tinymist/pull/460
  * Fixed some bugs in the vscode-variable package.
  * Improving the performance of replacing variables a bit.

### Compiler

* (Fix) Processing lagged compile reason in https://github.com/Myriad-Dreamin/tinymist/pull/456
  * Causing last key strokes not being processed correctly.

### Preview

* Modified static host to send Content-Type: text/html by @cskeeters in https://github.com/Myriad-Dreamin/tinymist/pull/465
  * Causing that GitHub Codespaces and the browser just showed the text of the HTML.

### Completion

* Supported querying label with paper name in bib items by @kririae in https://github.com/Myriad-Dreamin/tinymist/pull/365
* Added documentation about completion in https://github.com/Myriad-Dreamin/tinymist/pull/466

### Syntax/Semantic Highlighting

* Added syntax highlighting for raw blocks in https://github.com/Myriad-Dreamin/tinymist/pull/450
  * To ensure 100% correctness of grammar, only the raw block with number fence ticks less than 6 is highlighted.

### Misc

* Handling unwrap for the args in compile command by @upsidedownsweetfood in https://github.com/Myriad-Dreamin/tinymist/pull/445

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.11.16...v0.11.17

## v0.11.16 - [2024-07-20]

* Adding editor-side e2e testing in https://github.com/Myriad-Dreamin/tinymist/pull/441 and https://github.com/Myriad-Dreamin/tinymist/pull/442

### Compiler

* Making compilation not block most snapshot requests in https://github.com/Myriad-Dreamin/tinymist/pull/432 and https://github.com/Myriad-Dreamin/tinymist/pull/435
* Making cache evicting shared in https://github.com/Myriad-Dreamin/tinymist/pull/434
  * To make more sensible cache eviction when you are previewing multiple documents (running multiple compilers).
* (Fix) Changing entry if pinning again in https://github.com/Myriad-Dreamin/tinymist/pull/430
  * This was introduced by https://github.com/Myriad-Dreamin/tinymist/pull/406
* (Fix) Tolerating client changing source state badly in https://github.com/Myriad-Dreamin/tinymist/pull/429
  * Sometimes the client sends a request with a wrong source state, which causes a panic.

### Editor

* Showing views only if tinymist extension is activated in https://github.com/Myriad-Dreamin/tinymist/pull/420
  * This is a slightly improvement on https://github.com/Myriad-Dreamin/tinymist/pull/414
* (Fix) Removed dirty preview command changes in https://github.com/Myriad-Dreamin/tinymist/pull/421
  * It also adds dev kit to avoid this stupid mistake in future. The kit contains a convenient command for previewing document on a fixed port for development.
* Added hint documentation about configuring rootless document in https://github.com/Myriad-Dreamin/tinymist/pull/440
  * You can set the rootPath to `-`, so that tinymist will always use parent directory of the file as the root path.

### Commands/Tools

* Supported creation-timestamp configuration for exporting PDF in https://github.com/Myriad-Dreamin/tinymist/pull/439
  * It now start to provide creation timestamp for the PDF export.
  * You can disallow it to embed the creation timestamp in your document by `set document(..)`.
  * You can also configure it by either [Passing Extra CLI Arguments](https://github.com/Myriad-Dreamin/tinymist/blob/9ceae118480448a5ef0c41a1cf810fa1a072420e/editors/vscode/README.md#passing-extra-cli-arguments) or the environment variable (`SOURCE_DATE_EPOCH`).
    * For more details, please see [source-date-epoch](https://reproducible-builds.org/specs/source-date-epoch/)

### Preview

* Allowing multiple-tasked preview in https://github.com/Myriad-Dreamin/tinymist/pull/427
* Provided `sys.inputs.x-preview` in https://github.com/Myriad-Dreamin/tinymist/pull/438
  * It could be used for customizing the templates when you are previewing documents.

### Completion

* (Fix) Check string's quote prefix correctly when completing code in https://github.com/Myriad-Dreamin/tinymist/pull/422


### Misc

* Fixed description for exportPdf setting by @Otto-AA in https://github.com/Myriad-Dreamin/tinymist/pull/431

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.11.15...v0.11.16

## v0.11.15 - [2024-07-15]

* Bumped typstyle to v0.11.30 by @Enter-tainer in https://github.com/Myriad-Dreamin/tinymist/pull/416

### Compiler

* (Fix) Noting `formatter_print_width` change on changed configuration in https://github.com/Myriad-Dreamin/tinymist/pull/387
* Keeping entry on language query in https://github.com/Myriad-Dreamin/tinymist/pull/406
* Allowed deferred snapshot event processing in https://github.com/Myriad-Dreamin/tinymist/pull/408

### Editor

* (Fix) Showing views in activity bar whenever the extension is activated in https://github.com/Myriad-Dreamin/tinymist/pull/414

### Hover (Tooltip)

* Rendering example code in typst docs as typst syntax in https://github.com/Myriad-Dreamin/tinymist/pull/397

### Preview

* Using `requestIdleCallback` to wait for updating canvas pages when editor is in idle in https://github.com/Myriad-Dreamin/tinymist/pull/412
  * Improve performance when updating document quickly.
* (Fix) Fixed some corner cases of serving preview in https://github.com/Myriad-Dreamin/tinymist/pull/385
* (Fix) Scrolling source correctly when no text editor is active in https://github.com/Myriad-Dreamin/tinymist/pull/395
* (Fix) Updating content preview incrementally again in https://github.com/Myriad-Dreamin/tinymist/pull/413
* (Fix) wrong serialization of `task_id` v.s. `taskId` in https://github.com/Myriad-Dreamin/tinymist/pull/417

### Misc

* Added typlite for typst's doc comments in https://github.com/Myriad-Dreamin/tinymist/pull/398
* Documented tinymist crate in https://github.com/Myriad-Dreamin/tinymist/pull/390
* (Fix) Performing cyclic loop dependence correctly when checking def-use relation across module in https://github.com/Myriad-Dreamin/tinymist/pull/396

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.11.14...v0.11.15

## v0.11.14 - [2024-07-07]

## Compiler

This bug is introduced by [Preparing for parallelizing lsp requests](https://github.com/Myriad-Dreamin/tinymist/pull/342).

* (Fix) Lsp should respond errors at tail in https://github.com/Myriad-Dreamin/tinymist/pull/367

### Commands/Tools

* Supported single-task preview commands in https://github.com/Myriad-Dreamin/tinymist/pull/364, https://github.com/Myriad-Dreamin/tinymist/pull/368, https://github.com/Myriad-Dreamin/tinymist/pull/370, and https://github.com/Myriad-Dreamin/tinymist/pull/371
  * Typst Preview extension is already integrated into Tinymist. It . Please disable Typst Preview extension to avoid conflicts.
  * Otherwise, you should disable the tinymist's embedded preview feature by `"tinymist.preview": "disable"` in your settings.json.

### Preview

* Persisting webview preview through vscode restarts and @noamzaks in https://github.com/Myriad-Dreamin/tinymist/pull/373

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.11.13...v0.11.14

## v0.11.13 - [2024-07-02]

## Compiler

These bugs are introduced by [Preparing for parallelizing lsp requests](https://github.com/Myriad-Dreamin/tinymist/pull/342).

* (Fix) diagnostics is back in https://github.com/Myriad-Dreamin/tinymist/pull/354
* (Fix) Checking main before compilation in https://github.com/Myriad-Dreamin/tinymist/pull/361

## Misc
* Optimized release profile in https://github.com/Myriad-Dreamin/tinymist/pull/359

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.11.12...v0.11.13

## v0.11.12 - [2024-06-27]

* Bumped typstyle to v0.11.28
* Added base documentation website in https://github.com/Myriad-Dreamin/tinymist/pull/344 and https://github.com/Myriad-Dreamin/tinymist/pull/345

### Compiler

* Preparing for parallelizing lsp requests in https://github.com/Myriad-Dreamin/tinymist/pull/342

### Commands/Tools

* Added font list export panel in summary tool by @7sDream in https://github.com/Myriad-Dreamin/tinymist/pull/322

### Syntax/Semantic Highlighting

* Disabling bracket colorization in markup mode in https://github.com/Myriad-Dreamin/tinymist/pull/346
* (Fix) Terminating expression before math blocks in https://github.com/Myriad-Dreamin/tinymist/pull/347

### Completion

* (Fix) Avoided duplicated method completion in https://github.com/Myriad-Dreamin/tinymist/pull/349
* Fixed a bad early return in param_completions in https://github.com/Myriad-Dreamin/tinymist/pull/350
* Fixed completion in string context a bit in https://github.com/Myriad-Dreamin/tinymist/pull/351
  * It can handle empty string literals correctly now.
  * The half-completed string literals still have a problem though.

### Misc

* Moved typst-preview to tinymist and combined the binary and compiler in https://github.com/Myriad-Dreamin/tinymist/pull/323, https://github.com/Myriad-Dreamin/tinymist/pull/332, and https://github.com/Myriad-Dreamin/tinymist/pull/337

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.11.11...v0.11.12

## v0.11.11 - [2024-06-17]

* Bumped typstyle to v0.11.26 by @Enter-tainer in https://github.com/Myriad-Dreamin/tinymist/pull/326

## Compiler

* (Fix): Handling the conversion of offset at the EOF in https://github.com/Myriad-Dreamin/tinymist/pull/325
* (Fix) Accumulating export events correctly in https://github.com/Myriad-Dreamin/tinymist/pull/330

## Document Highlighting (New)

* Highlighting all break points for that loop context in https://github.com/Myriad-Dreamin/tinymist/pull/317

## On Enter (New)

* Implemented `experimental/onEnter` in https://github.com/Myriad-Dreamin/tinymist/pull/328

## Completion

* Generating names for destructuring closure params by @wrenger in https://github.com/Myriad-Dreamin/tinymist/pull/319

## Misc

* Combined CompileClient and CompileClientActor by @QuarticCat in https://github.com/Myriad-Dreamin/tinymist/pull/318
* Simplified pin_entry by @QuarticCat in https://github.com/Myriad-Dreamin/tinymist/pull/320

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.11.10...v0.11.11

## v0.11.10 - [2024-05-26]

* Bumped typstyle to v0.11.23 by @Enter-tainer in https://github.com/Myriad-Dreamin/tinymist/pull/315

### Editor

* Transparentized the background of typst icon in https://github.com/Myriad-Dreamin/tinymist/pull/313
* Made more consistent font configuration in https://github.com/Myriad-Dreamin/tinymist/pull/312

### Completion

* Completing CSL paths in https://github.com/Myriad-Dreamin/tinymist/pull/310

### Code Action
* Checking and moving the exactly single punctuation after the math equation to refactor in https://github.com/Myriad-Dreamin/tinymist/pull/306

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.11.9...v0.11.10

## v0.11.9 - [2024-05-18]

* Bumped typst to 0.11.1 in https://github.com/Myriad-Dreamin/tinymist/pull/301
* Bumped to typstyle v0.11.21 by @Enter-tainer in https://github.com/Myriad-Dreamin/tinymist/pull/303
* Upgraded rust and set MSRV to 1.75 in https://github.com/Myriad-Dreamin/tinymist/pull/261

* Documented overview of tinymist in https://github.com/Myriad-Dreamin/tinymist/pull/274, https://github.com/Myriad-Dreamin/tinymist/pull/276, and https://github.com/Myriad-Dreamin/tinymist/pull/295

### Editor

* (Fix) Implicitly focusing entry on no focus request sent in https://github.com/Myriad-Dreamin/tinymist/pull/262
* Linking documentation to typst.zed for zed users in https://github.com/Myriad-Dreamin/tinymist/pull/268

### Compiler

* (Fix) Corrected order of def-and-use for named params in https://github.com/Myriad-Dreamin/tinymist/pull/281

### AST Matchers

* (Fix) Searching newline character in utf-8 bytes sequence with tolerating unaligned access in https://github.com/Myriad-Dreamin/tinymist/pull/299
* (Fix) Gets targets to check or deref without skip trivia node in non-code context in https://github.com/Myriad-Dreamin/tinymist/pull/289
* (Fix) Determining `is_set` for checking targets in https://github.com/Myriad-Dreamin/tinymist/pull/286

### Commands/Tools

* Resolved symbols for Symbol View Tool in compile-based approach in https://github.com/Myriad-Dreamin/tinymist/pull/269
  * It is more robust and flexible than the previous approach.

### Completion

* (Fix) properly stops call expressions in https://github.com/Myriad-Dreamin/tinymist/pull/273
* (Fix) completion path with ctx.leaf in https://github.com/Myriad-Dreamin/tinymist/pull/282
* (Fix) filter unsettable params when making a set rule in https://github.com/Myriad-Dreamin/tinymist/pull/287
* Removed literal themselves for completion in https://github.com/Myriad-Dreamin/tinymist/pull/291
  - e.g. `#let x = (1.);`. it was completing `1.0`, which is funny.
* Completing both open and closed labels in https://github.com/Myriad-Dreamin/tinymist/pull/302

### Signature Help

* (Fix) Matching labels in signature help correctly in https://github.com/Myriad-Dreamin/tinymist/pull/288

### Code Action

* Added simple code actions to manipulate equations in https://github.com/Myriad-Dreamin/tinymist/pull/258

### Formatting

* Fixed suffix computation by @QuarticCat in https://github.com/Myriad-Dreamin/tinymist/pull/263

### Misc

* Installing detypify service from npm in https://github.com/Myriad-Dreamin/tinymist/pull/275 and https://github.com/Myriad-Dreamin/tinymist/pull/277
* Implemented naive substitution for types (β-reduction) in https://github.com/Myriad-Dreamin/tinymist/pull/292

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.11.8...v0.11.9

## v0.11.8 - [2024-05-07]

### Hover

* Improved open document tooltip in https://github.com/Myriad-Dreamin/tinymist/pull/254

### Completion

* Inserting commas in argument context for completing before identifiers in https://github.com/Myriad-Dreamin/tinymist/pull/251
* Improved identifying literal expressions in https://github.com/Myriad-Dreamin/tinymist/pull/252
* Identifying let context completely in https://github.com/Myriad-Dreamin/tinymist/pull/255
  * To help complete after equal marker in `let b = ..`
* Restoring left parenthesis and comma as trigger characters in https://github.com/Myriad-Dreamin/tinymist/pull/253
  * This is needed for completion on literal expressions.

### Type Checking

* (Fix) Avoiding infinite loop in simplifying recursive functions in https://github.com/Myriad-Dreamin/tinymist/pull/246
  * Fix a stack overflow in `ty.rs`
* (Fix) Instantiating variable before applying variable function in https://github.com/Myriad-Dreamin/tinymist/pull/247
  * Fix a deadlock in `ty.rs`
* (Fix) Simplifying all substructure in https://github.com/Myriad-Dreamin/tinymist/pull/248
  * Fix a panic in `ty.rs`
* Improved join type inference in https://github.com/Myriad-Dreamin/tinymist/pull/249
* Weakening inference from outer use in https://github.com/Myriad-Dreamin/tinymist/pull/250
  * to reduce noise slightly for completion

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.11.7...v0.11.8

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
* Showing label descriptions according to types in https://github.com/Myriad-Dreamin/tinymist/pull/237
* Filtering completions by module import in https://github.com/Myriad-Dreamin/tinymist/pull/234
* Filtering completions by surrounding syntax for elements/selectors in https://github.com/Myriad-Dreamin/tinymist/pull/236

### Code Action (New)

* Provided code action to rewrite headings in https://github.com/Myriad-Dreamin/tinymist/pull/240

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

* (Fix) parse docstring dedents correctly in https://github.com/Myriad-Dreamin/tinymist/pull/132

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
