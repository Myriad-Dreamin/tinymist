# Change Log

All notable changes to the "tinymist" extension will be documented in this file.

Check [Keep a Changelog](http://keepachangelog.com/) for recommendations on how to structure this file.

The changelog lines unspecified with authors are all written by the @Myriad-Dreamin.

## v0.13.12 - [2025-04-18]

* Bumped typstyle from v0.13.1 to v0.13.3 in https://github.com/Myriad-Dreamin/tinymist/pull/1651
  * This version achieves full document formatting support. It now comprehensively processes previously skipped elements, such as markup lines mixed with equations or codes, equations with comments, math expressions containing `#` symbols, and math arguments. There are also a few minor bug fixes and enhancements related to equations and import items. For more details, see https://enter-tainer.github.io/typstyle/changelog/#v0133---2025-04-10.
* Bumped world crates to 0.13.12-rc1 in https://github.com/Myriad-Dreamin/tinymist/pull/1608
* todo: hard disable targets to build on CI in https://github.com/Myriad-Dreamin/tinymist/pull/1613

### Server

* Hot updating configuratuion item `tinymist.compileStatus` in https://github.com/Myriad-Dreamin/tinymist/pull/1584
* Supporting `--feature` and `--pdf-standard` in `typstExtraArgs` in https://github.com/Myriad-Dreamin/tinymist/pull/1596
* feat: resolve roots of typst packages in https://github.com/Myriad-Dreamin/tinymist/pull/1663

### Compiler

* (Perf) Detecting root change correctly in https://github.com/Myriad-Dreamin/tinymist/pull/1661
  * This was invalidating vfs cache frequently.
* Removed system time deps from crates in https://github.com/Myriad-Dreamin/tinymist/pull/1621
  * This allows tinymist to build to `wasm32-unknown-unknown` target, which is required to use tinymist as a typst plugin.

### Editor

* (Fix) Corrected `tokenTypes` of math quotes from `string` to `other` in https://github.com/Myriad-Dreamin/tinymist/pull/1618
  * When typing on `$|$`, it was not completing `""` correctly since the editor thought `$$` are string and the cursor is in a string.
* (Perf) Delaying focus change to typst documents in https://github.com/Myriad-Dreamin/tinymist/pull/1662
  * This was invalidating vfs cache frequently when you switch document by APIs like "goto definition".
* (Change) Changing configuratuion item `tinymist.formatterMode`'s default value from `never` to `typstyle` by @kaerbr in https://github.com/Myriad-Dreamin/tinymist/pull/1655
* Supporting to use `{pageCount}` in `tinymist.statusBarFormat` in https://github.com/Myriad-Dreamin/tinymist/pull/1666
* Providing AST view in https://github.com/Myriad-Dreamin/tinymist/pull/1617

### Export

* Atomically writing compilation artifacts by @seven-mile in https://github.com/Myriad-Dreamin/tinymist/pull/1586
  * For PDF export, PDF files was clearing the content and writing directly. PDF viewers may be unhappy when reading a half-complete content.

### Code Analysis

* (Fix) Resolving relative path in subfolders in https://github.com/Myriad-Dreamin/tinymist/pull/1574
  * This fixes document links in source files located in subfolders.
* (Fix) Corrected rename on unix platforms caused by pathdiff#8 in https://github.com/Myriad-Dreamin/tinymist/pull/1587
  * This fixes renames on relative imports like `#import "../foo.typ"`.
* (Fix) Corrected `jump_from_cursor` and add tests in https://github.com/Myriad-Dreamin/tinymist/pull/1589
  * This fixes jumps from math text in source panel to the preview panel.
* (Fix) Tolerating the fact that plugin functions don't have parameters in https://github.com/Myriad-Dreamin/tinymist/pull/1605
  * This was causing panels when completing plugin functions.
* (Fix) Corrected `name_range` implementation in https://github.com/Myriad-Dreamin/tinymist/pull/1623
  * This was causing the issue when hovering bibliography items.
* Checking field of literals in https://github.com/Myriad-Dreamin/tinymist/pull/1619
  * This was causing the issue when code completing methods of literals.

### Linting (New)

* Linting on bug-prone show/set rules in https://github.com/Myriad-Dreamin/tinymist/pull/1634
* Linting implicitly discarded statements before `break/continue/return` in https://github.com/Myriad-Dreamin/tinymist/pull/1637, https://github.com/Myriad-Dreamin/tinymist/pull/1664, and https://github.com/Myriad-Dreamin/tinymist/pull/1668
* Linting types comparing with strings in https://github.com/Myriad-Dreamin/tinymist/pull/1643
  * warning on `type("") == "str"` which will be always false in future typst.
* Linting variable font uses by @Enter-tainer in https://github.com/Myriad-Dreamin/tinymist/pull/1649
  * warning on argument like `text(font: "XXX VF")` which isn't properly supported by typst.
* Providing `tinymist.lint.enabled` and `tinymist.lint.when` to disable or lint `on{Save,Type}` in https://github.com/Myriad-Dreamin/tinymist/pull/1658

### Preview

* (Fix) Dragging preview panel horizontally by @zica87 in https://github.com/Myriad-Dreamin/tinymist/pull/1597
* (Fix) Clearing selection on clicking on empty area by @zica87 in https://github.com/Myriad-Dreamin/tinymist/pull/1644
* Updated commands to scroll or kill all preview panels in https://github.com/Myriad-Dreamin/tinymist/pull/1451
* Ejecting preview panel to browser by @seven-mile in https://github.com/Myriad-Dreamin/tinymist/pull/1575

### Hover

* (Fix) Corrected links to official reference pages in hover docs in https://github.com/Myriad-Dreamin/tinymist/pull/1641
* Showing rendered bibliography and improving label formatting @QuadnucYard in https://github.com/Myriad-Dreamin/tinymist/pull/1611

### Definition

* Resolving full ranges of bibliography items in https://github.com/Myriad-Dreamin/tinymist/pull/1627
  * To help show bibliography items when "ctrl" hover on the references to bibliography.

### Folding Range

* Folding `list` and `enum` items by @BlueQuantumx in https://github.com/Myriad-Dreamin/tinymist/pull/1598

### Diagnostics

* Removed extra line breaks in diagnostic message by @QuadnucYard in https://github.com/Myriad-Dreamin/tinymist/pull/1599

### Document Highlighting

* `context {}` breaking association of `break`/`continue` with parent loops in https://github.com/Myriad-Dreamin/tinymist/pull/1635
  * It was highlighting `while` when the cursor is on `break` in `while { context { break } }`.

### Misc

* VS Code extensions uses binaries built by cargo-dist in https://github.com/Myriad-Dreamin/tinymist/pull/1560
* Running e2e tests on major platforms in https://github.com/Myriad-Dreamin/tinymist/pull/1590
* Building and bundling tinymist's PDF docs in VS Code extensions for all platforms in https://github.com/Myriad-Dreamin/tinymist/pull/1592
* Using typst's html export to render tinymist's online docs in https://github.com/Myriad-Dreamin/tinymist/pull/1610
* Added sponsoring section to readme in https://github.com/Myriad-Dreamin/tinymist/pull/1620
* Updated Neovim config to use non-blocking system call by @ptdewey in https://github.com/Myriad-Dreamin/tinymist/pull/1607
* Fixed syntax error in Neovim docs by @ptdewey in https://github.com/Myriad-Dreamin/tinymist/pull/1672

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.13.10...v0.13.12

## v0.13.10 - [2025-03-23]

* Bumped typst to v0.13.1 in https://github.com/Myriad-Dreamin/tinymist/pull/1540
* Bumped typstfmt to v0.13.1 in https://github.com/Myriad-Dreamin/tinymist/pull/1540

### CLI

* Only keeping diagnostics message in the compile command in https://github.com/Myriad-Dreamin/tinymist/pull/1512
* Added `tinymist test` command with coverage support in https://github.com/Myriad-Dreamin/tinymist/pull/1518 and https://github.com/Myriad-Dreamin/tinymist/pull/1535
* Allowing to watch tests in https://github.com/Myriad-Dreamin/tinymist/pull/1534

### Editor

* Pasting URI smartly in https://github.com/Myriad-Dreamin/tinymist/pull/1500
  * If nothing is selected, it will generate a link element in place respecting the markup/math/code mode under the cursor.
  * If the selected range is a link, it will simply update the link and not generate a string.
  * Otherwise, the selected range is wrapped as the content of the link element.
* Downgrading some errors in the configurations and showing warnings by popping up message window in https://github.com/Myriad-Dreamin/tinymist/pull/1538
  * Previously, if there is an error in the configuration, all the configuration items will have no effect.
* Configuring word pattern to not matching words like `-A` in https://github.com/Myriad-Dreamin/tinymist/pull/1552
* Making all export features available by commands in https://github.com/Myriad-Dreamin/tinymist/pull/1547

### Testing

* Implemented debugging console in https://github.com/Myriad-Dreamin/tinymist/pull/1517 and https://github.com/Myriad-Dreamin/tinymist/pull/1445
* Implemented software breakpoint instrumentation in https://github.com/Myriad-Dreamin/tinymist/pull/1529
* Profiling and visualizing coverage of the current document in https://github.com/Myriad-Dreamin/tinymist/pull/1490
* Profiling and visualizing test coverage of the current module in https://github.com/Myriad-Dreamin/tinymist/pull/1518, https://github.com/Myriad-Dreamin/tinymist/pull/1532, https://github.com/Myriad-Dreamin/tinymist/pull/1533, and https://github.com/Myriad-Dreamin/tinymist/pull/1535

### Localization

* Translated all titles and descriptions of tinymist vscode commands using LLM in https://github.com/Myriad-Dreamin/tinymist/pull/1501, https://github.com/Myriad-Dreamin/tinymist/pull/1502, https://github.com/Myriad-Dreamin/tinymist/pull/1503, and https://github.com/Myriad-Dreamin/tinymist/pull/1504
* Translated some code lens titles and error messages in tinymist-cli using LLM in https://github.com/Myriad-Dreamin/tinymist/pull/1505, https://github.com/Myriad-Dreamin/tinymist/pull/1507, and https://github.com/Myriad-Dreamin/tinymist/pull/1508

### Export

* (Fix) Allowing HTML export when the server is configured under `paged` export target and vice versa in https://github.com/Myriad-Dreamin/tinymist/pull/1549
* Added vscode E2E testing for export features in https://github.com/Myriad-Dreamin/tinymist/pull/1553

### Diagnostics

* Added diagnostics refiner to edit or provide hints from tinymist side by @seven-mile in https://github.com/Myriad-Dreamin/tinymist/pull/1539 and https://github.com/Myriad-Dreamin/tinymist/pull/1544

### Code Analysis

* (Fix) Correctly checking wildcard import in https://github.com/Myriad-Dreamin/tinymist/pull/1563

### Completion

* (Fix) Reverted the explicit detection again in https://github.com/Myriad-Dreamin/tinymist/pull/1525
* (Fix) Corrected bound self checking in https://github.com/Myriad-Dreamin/tinymist/pull/1564
* Forbidding bad field access completion in math mode in https://github.com/Myriad-Dreamin/tinymist/pull/1550
* Forbidding bad postfix completion in math mode in https://github.com/Myriad-Dreamin/tinymist/pull/1556
* Not triggering parameter hints when skipping parameters in https://github.com/Myriad-Dreamin/tinymist/pull/1557

### Preview

* (Security) Made more strict CORS checks (v2) by @tmistele in https://github.com/Myriad-Dreamin/tinymist/pull/1382
* Using `window/showDocument` to show previewing document in https://github.com/Myriad-Dreamin/tinymist/pull/1450

### Misc

* Updated roadmap in https://github.com/Myriad-Dreamin/tinymist/pull/1499
* Fixed Neovim name casing everywhere by @Andrew15-5 in https://github.com/Myriad-Dreamin/tinymist/pull/1520
* Fixed build scripts by @Andrew15-5 in https://github.com/Myriad-Dreamin/tinymist/pull/1522

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.13.8...v0.13.10

## v0.13.8 - [2025-03-13]

### Completion

* (Fix) More rules to forbidden arg completion in https://github.com/Myriad-Dreamin/tinymist/pull/1493
  * It were completing arguments from `#align[]|` or `#align()[]|`
* (Fix) Don't check context type if parent is a block in https://github.com/Myriad-Dreamin/tinymist/pull/1494
  * It were completing arguments from `#align[|]`, `#align([|])`, or `#align({|})`
* (Fix) Forbid some bad cases of dot access in https://github.com/Myriad-Dreamin/tinymist/pull/1497
  * It were issuing postfix completion from `$.|$` or `$ .| $`
* Detecting explicit completion from vscode in https://github.com/Myriad-Dreamin/tinymist/pull/1496
  * Requesting completion about `$|$` or `$abs(a)|$` took no effect.

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.13.6...v0.13.8

## v0.13.6 - [2025-03-13]

We has provided more ways of previewing documents for editors having poor lsp support.
- Default Preview: The editors supporting lsp commands, e.g. Neovim and helix, can use [`tinymist.startDefaultPreview`](https://myriad-dreamin.github.io/tinymist/feature/preview.html#label-default-preview) to start a browsing preview server directly.
- Background Preview: The editors not supporting lsp commands can use the [background preview](https://myriad-dreamin.github.io/tinymist/feature/preview.html#label-background-preview) feature to start a preview server in background. You can bind a shortcut editor to open the preview in browser.

See the [Issue: Preview feature for all editors](https://github.com/Myriad-Dreamin/tinymist/issues/1237) for unimplemented features.

* Provided tinymist documentation in PDF format in https://github.com/Myriad-Dreamin/tinymist/pull/1485

### Compiler

* (Fix) Getting task options from configuration in https://github.com/Myriad-Dreamin/tinymist/pull/1449
* (Fix) Displaying `ProjectInsId` without quoting in https://github.com/Myriad-Dreamin/tinymist/pull/1476
  * This made document summary not working.
* (Perf) Parallelizing and synchronously waiting font loading in https://github.com/Myriad-Dreamin/tinymist/pull/1470

### Code Analysis

* (Fix) Identifying chained dot access when matching atomic expression in markup mode in https://github.com/Myriad-Dreamin/tinymist/pull/1488 and https://github.com/Myriad-Dreamin/tinymist/pull/1489
  * When completing `#a.b.|`, the second `.` was viewed as a text dot and failed to trigger the field completion. It now reparses correctly.
* Made file type recognition by file extension case-insensitive in https://github.com/Myriad-Dreamin/tinymist/pull/1472
  * For example, `IMAGE.PNG` is recognized as an image file now.

### Editor

* (Fix) Combining VS Code language specific default settings into one block by @0risc in https://github.com/Myriad-Dreamin/tinymist/pull/1462

### Completion

* (Fix) Skipping argument completion when the cursor is on the right parenthesis in https://github.com/Myriad-Dreamin/tinymist/pull/1480
* (Fix) Distinguished content value from content type in https://github.com/Myriad-Dreamin/tinymist/pull/1482
  * `math.op("+")` was wrongly inferred as an element function (type), instead of a value having the element type.
* Adjusting range of label and reference completions in https://github.com/Myriad-Dreamin/tinymist/pull/1443 and https://github.com/Myriad-Dreamin/tinymist/pull/1444
  * It becomes more sensible when you request completions from anywhere on the labels or references.
* Unifying and improving function and method completion in https://github.com/Myriad-Dreamin/tinymist/pull/1478
  * The was affecting `show outline.entry`. It was completing `e|` as `entry()` instead of `entry`.
* Skip completion of types having no constructors or scopes in https://github.com/Myriad-Dreamin/tinymist/pull/1481
  * For example, `content` is not completed.
* Completing `std` module in https://github.com/Myriad-Dreamin/tinymist/pull/1483
  * `std` is in neither global scope nor math scope, so we have to handle it manually.
* Accepting arbitrary expressions in show rules in https://github.com/Myriad-Dreamin/tinymist/pull/1484
  * For example, `show: s|` now can be completed as `show: std|`, and so that further completed as `show: std.scale(..)`. It was not working because modules were filtered out as not a valid show transform function.

### Preview

* Added support to run preview server in background in https://github.com/Myriad-Dreamin/tinymist/pull/1233
* Added `tinymist.startDefaultPreview` and revised documentation about preview in https://github.com/Myriad-Dreamin/tinymist/pull/1448

### Misc

* Updated bug report and feature request template in https://github.com/Myriad-Dreamin/tinymist/pull/1454, https://github.com/Myriad-Dreamin/tinymist/pull/1455, https://github.com/Myriad-Dreamin/tinymist/pull/1456, https://github.com/Myriad-Dreamin/tinymist/pull/1457, and https://github.com/Myriad-Dreamin/tinymist/pull/1458
* Logging `update_by_map` to debug zed configuration in https://github.com/Myriad-Dreamin/tinymist/pull/1474

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.13.4...v0.13.6

## v0.13.4 - [2025-03-02]

### Code Analysis

* (Fix) Skipping context type checking of hash token in https://github.com/Myriad-Dreamin/tinymist/pull/1432

### Preview

* (Fix) Using the background rect to calculate cursor
position in the page in https://github.com/Myriad-Dreamin/tinymist/pull/1427

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.13.2...v0.13.4

## v0.13.2 - [2025-02-27]

* Bumped MSRV to v1.83 and Rust to v1.85 in https://github.com/Myriad-Dreamin/tinymist/pull/1407
* Bumped typst-ansi-hl to v0.4.0 in https://github.com/Myriad-Dreamin/tinymist/pull/1412
* Bumped reflexo to v0.5.5-rc7 in https://github.com/Myriad-Dreamin/tinymist/pull/1414

### Editor

* (Fix) Deactivating features correctly when restarting server in https://github.com/Myriad-Dreamin/tinymist/pull/1397
* Making `tinymist.configureDefaultWordSeparator` opt in, as discussed, in https://github.com/Myriad-Dreamin/tinymist/pull/1389

### Compiler

* (Fix) Letting `tinymist::Config` pull environment variables on start of server in https://github.com/Myriad-Dreamin/tinymist/pull/1390
* Tested that `TYPST_PACKAGE_CACHE_PATH` should be applied on server start in https://github.com/Myriad-Dreamin/tinymist/pull/1391

### CLI

* (Fix) Ensured `tinymist-cli`'s argument names unique in https://github.com/Myriad-Dreamin/tinymist/pull/1388
* Added test about completion script generation in https://github.com/Myriad-Dreamin/tinymist/pull/1387

### Code Analysis

* (Fix) term math text as content instead of string in https://github.com/Myriad-Dreamin/tinymist/pull/1386
* (Fix) Printing type representation of anonymous modules in https://github.com/Myriad-Dreamin/tinymist/pull/1385
  * This was causing crashes, introduced in typst v0.13.0
* (Fix) Added more kind checking about `MathText` in https://github.com/Myriad-Dreamin/tinymist/pull/1415
* (Fix) Completing type of type having constructors in https://github.com/Myriad-Dreamin/tinymist/pull/1419
* (Fix) Forbidden type completion in string content in https://github.com/Myriad-Dreamin/tinymist/pull/1420
* Adjusted builtin types for typst v0.13.0 in https://github.com/Myriad-Dreamin/tinymist/pull/1416
  * For example, `par.first-line-indent` can be a dictionary since v0.13.0.
* Post checking element types of array and dictionary in https://github.com/Myriad-Dreamin/tinymist/pull/1417
* Matched named argument parent first when the cursor is in the literal in https://github.com/Myriad-Dreamin/tinymist/pull/1418

### Preview

* (Fix) Uses new wasm renderer in https://github.com/Myriad-Dreamin/tinymist/pull/1398
* (Fix) Corrected `vscode.Uri` usages when restoring preview in https://github.com/Myriad-Dreamin/tinymist/pull/1402
* (Fix) Passing origin checking anyway in https://github.com/Myriad-Dreamin/tinymist/pull/1411
  * typst-preview.nvim uses a websocket client without setting Origin correctly.
  * This will become a hard error in the future.
* Using `jump_from_click` from typst-ide in https://github.com/Myriad-Dreamin/tinymist/pull/1399

### Typlite

* (Fix) Exposed and defaulted to no-content-hint in typlite by @selfisekai in https://github.com/Myriad-Dreamin/tinymist/pull/1381
* Added examples for `--assets-path` and `--assets-src-path` by @hongjr03 in https://github.com/Myriad-Dreamin/tinymist/pull/1396

### Syntax Highlighting

* Parsing Shebang syntax in https://github.com/Myriad-Dreamin/tinymist/pull/1400
* Recognizing typst source files by shebang containing `typst` keyword in https://github.com/Myriad-Dreamin/tinymist/pull/1400
  * Specifically, it matches by regex `^#!/.*\\b(typst)[0-9.-]*\\b`

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.13.0...v0.13.2

## v0.13.0 - [2025-02-23]

* Bumped typst to v0.13.0 in https://github.com/Myriad-Dreamin/tinymist/pull/1342 and https://github.com/Myriad-Dreamin/tinymist/pull/1361
* Bumped typstyle to v0.13.0 in https://github.com/Myriad-Dreamin/tinymist/pull/1368

### Editor

* feat: initialize tinymist-vscode-html extension in https://github.com/Myriad-Dreamin/tinymist/pull/1378

### HTML Export

* Providing `tinymist.exportTarget` for running language server targeting html in https://github.com/Myriad-Dreamin/tinymist/pull/1284
  * `tinymist.exportTarget` is the target to export the document to.
  * Use `paged` (default): The current export target is for PDF, PNG, and SVG export.
  * Use `html`: The current export target is for HTML export.
* Exporting text (`.txt`) over typst's HTML export in https://github.com/Myriad-Dreamin/tinymist/pull/1289
  * This is used for word count and `tinymist.exportText`.

### Misc

* Published {tinymist-{derive,analysis,std,vfs,world,project},typlite,crityp} crates to crates.io (#1310)
* Mentioning script to download nightly prebuilts in https://github.com/Myriad-Dreamin/tinymist/pull/1377

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.12.20...v0.13.0

## v0.12.22 - [2025-02-23]

### Compiler

* (Fix) Removing diagnostics when removing a project in https://github.com/Myriad-Dreamin/tinymist/pull/1372
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

## v0.12.21 - [2025-02-20]

Nightly Release at [feat: split tinymist-task (#1277)](https://github.com/Myriad-Dreamin/tinymist/commit/3799db6dd4b3a6504fe295ff74d6e82cc57d16bf), using [ParaN3xus/typst tinymist-nightly-v0.12.21-content-hint](https://github.com/ParaN3xus/typst/tree/tinymist-nightly-v0.12.21-content-hint), a.k.a. [typst/typst 0.13 changelog (#5801)](https://github.com/typst/typst/commit/4a9a5d2716fc91f60734769eb001aef32fe15403).

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.12.19...v0.12.21

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
* (Fix) Emitting latest status and artifact with correct signals (#1294) in https://github.com/Myriad-Dreamin/tinymist/pull/1330
  * Because of this, the compile status bar was not updated correctly.
* (Perf) Detecting compilation-related vfs changes in https://github.com/Myriad-Dreamin/tinymist/pull/1199
* (Perf) Scatter-gathering the editor diagnostics in https://github.com/Myriad-Dreamin/tinymist/pull/1246
* Moved world implementation to tinymist in https://github.com/Myriad-Dreamin/tinymist/pull/1177, https://github.com/Myriad-Dreamin/tinymist/pull/1183, https://github.com/Myriad-Dreamin/tinymist/pull/1185, https://github.com/Myriad-Dreamin/tinymist/pull/1186, and https://github.com/Myriad-Dreamin/tinymist/pull/1187
* Reduced size of the watch entry in https://github.com/Myriad-Dreamin/tinymist/pull/1190 and https://github.com/Myriad-Dreamin/tinymist/pull/1191
* Tracking fine-grained revisions of `font`, `registry`, `entry`, and `vfs` in https://github.com/Myriad-Dreamin/tinymist/pull/1192
  * This prepares for better configuration hot reloading in future.
* Triggering project compilations on main thread in https://github.com/Myriad-Dreamin/tinymist/pull/1197
  * This helps apply more advanced compilation strategy with sync and mutable state on the main thread. For example, [Filtering out unrelated file changes](https://github.com/Myriad-Dreamin/tinymist/pull/1199) has been applied.
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
  * Completes multiple paths on `bibliography` and completes wasm files on `plugin`.

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

* Revised Neovim's install section by @SylvanFranklin and @YDX-2147483647 in https://github.com/Myriad-Dreamin/tinymist/pull/1090 and https://github.com/Myriad-Dreamin/tinymist/pull/1276
* Added release instruction by @ParaN3xus and @Myriad-Dreamin in https://github.com/Myriad-Dreamin/tinymist/pull/1163, https://github.com/Myriad-Dreamin/tinymist/pull/1169, https://github.com/Myriad-Dreamin/tinymist/pull/1173, and https://github.com/Myriad-Dreamin/tinymist/pull/1212
* Documenting `sync-lsp` crate in https://github.com/Myriad-Dreamin/tinymist/pull/1155
* CI used newest deploy-pages, upload-pages-artifact, and configure-pages actions in https://github.com/Myriad-Dreamin/tinymist/pull/1249 and https://github.com/Myriad-Dreamin/tinymist/pull/1251
* Documenting Myriad-Dreamin's workspace setting in https://github.com/Myriad-Dreamin/tinymist/pull/1264
* CI Added release crates action in https://github.com/Myriad-Dreamin/tinymist/pull/1298
 * Published {tinymist-{derive,analysis,std,vfs,world,project},typlite,crityp} crates in https://github.com/Myriad-Dreamin/tinymist/pull/1310

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.12.18...v0.12.20

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
