# Change Log

All notable changes to the "tinymist" extension will be documented in this file.

Check [Keep a Changelog](http://keepachangelog.com/) for recommendations on how to structure this file.

The changelog lines unspecified with authors are all written by the @Myriad-Dreamin.

- [CHANGELOG-2026.md](https://github.com/Myriad-Dreamin/tinymist/blob/main/editors/vscode/CHANGELOG.md)
- [CHANGELOG-2025.md](https://github.com/Myriad-Dreamin/tinymist/blob/main/CHANGELOG/CHANGELOG-2025.md)
- [CHANGELOG-2024.md](https://github.com/Myriad-Dreamin/tinymist/blob/main/CHANGELOG/CHANGELOG-2024.md)

## v0.14.10 - [2026-01-22]

### Server

* (Fix) Parsed path arguments of export commands exactly once in https://github.com/Myriad-Dreamin/tinymist/pull/2363
* (Fix) Specified and propagated "open" feature in https://github.com/Myriad-Dreamin/tinymist/pull/2365
  * This caused ineffective `--open` flag (https://github.com/Myriad-Dreamin/tinymist/issues/2361).

### Editor

* Added support for exporting markdown documents to PDF in https://github.com/Myriad-Dreamin/tinymist/pull/2241

### Code Analysis

* (Fix) Skipping non-source files for linting and references in https://github.com/Myriad-Dreamin/tinymist/pull/2348
* (Fix) Corrected `ModuleInclude` syntax identification in collecting links by @BlueQuantumx in https://github.com/Myriad-Dreamin/tinymist/pull/2364

### Preview

* Added hotkey to toggle light/dark theme by @funkeleinhorn in https://github.com/Myriad-Dreamin/tinymist/pull/2325

### Codelens

* Skipping emitting specific codelens for clients that don't have a client-side handler in https://github.com/Myriad-Dreamin/tinymist/pull/2246

### Misc

* Updated mason.nvim link in documentation by @gustavakerstrom in https://github.com/Myriad-Dreamin/tinymist/pull/2370

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.14.8...v0.14.10

## v0.14.8 - [2026-01-08]

### Rename

* Renaming labels (#1858) in https://github.com/Myriad-Dreamin/tinymist/pull/2133 and https://github.com/Myriad-Dreamin/tinymist/pull/2339

### Code Analysis

* Added hover tooltip for package import by @QuadnucYard in https://github.com/Myriad-Dreamin/tinymist/pull/2095

### Typlite

* (Fix) Corrected space handling among html element tags by @hongjr03 in https://github.com/Myriad-Dreamin/tinymist/pull/2243

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.14.6...v0.14.8

## v0.14.6 - [2026-01-02]

* Bumped typst to v0.14.2 in https://github.com/Myriad-Dreamin/tinymist/pull/2312

### Server

* Added `--input` flag to CLI commands by @xiyihan0 in https://github.com/Myriad-Dreamin/tinymist/pull/2328

### Editor

* Added character count format specifier to VS Code status bar by @Sejder in https://github.com/Myriad-Dreamin/tinymist/pull/2308
* Added new PDF options and image page number template to exporter by @QuadnucYard in https://github.com/Myriad-Dreamin/tinymist/pull/2281

### Code Analysis
* (Fix) Implemented total ordering for Typst values to ensure predictable sorting behavior in https://github.com/Myriad-Dreamin/tinymist/pull/2279
  * This fixes panics that occurred when comparing certain Typst values (Issue typst/typst#6285).
* (Fix) Added special syntax class for empty reference syntax in https://github.com/Myriad-Dreamin/tinymist/pull/2324
  * This fixes the issue that the text behind `@` was eaten on completion.
* Scanning namespaces in package directories in https://github.com/Myriad-Dreamin/tinymist/pull/2297 and https://github.com/Myriad-Dreamin/tinymist/pull/2313
* Storing full package information and caching local packages by @QuadnucYard in https://github.com/Myriad-Dreamin/tinymist/pull/2291
* Clearing local package read cache in https://github.com/Myriad-Dreamin/tinymist/pull/2299, https://github.com/Myriad-Dreamin/tinymist/pull/2298

### Preview

* (Fixed) fix intra-document links not working in preview by @ParaN3xus in https://github.com/Myriad-Dreamin/tinymist/pull/2287
  * This was introduced in https://github.com/Myriad-Dreamin/tinymist/pull/2145

### Misc

* (Fix) Changed checkOnSave to check in .zed/settings.json by @hongjr03 in https://github.com/Myriad-Dreamin/tinymist/pull/2314
* (Fix) Configured tokio feature set properly for tinymist-preview by @liquidhelium in https://github.com/Myriad-Dreamin/tinymist/pull/2323
* Added more formatting documentation for neovim by @PatrickMassot in https://github.com/Myriad-Dreamin/tinymist/pull/2322

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.14.4...v0.14.6
