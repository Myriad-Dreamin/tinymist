# Change Log

All notable changes to the "tinymist" extension will be documented in this file.

Check [Keep a Changelog](http://keepachangelog.com/) for recommendations on how to structure this file.

The changelog lines unspecified with authors are all written by the @Myriad-Dreamin.

- [CHANGELOG-2026.md](https://github.com/Myriad-Dreamin/tinymist/blob/main/editors/vscode/CHANGELOG.md)
- [CHANGELOG-2025.md](https://github.com/Myriad-Dreamin/tinymist/blob/main/CHANGELOG/CHANGELOG-2025.md)
- [CHANGELOG-2024.md](https://github.com/Myriad-Dreamin/tinymist/blob/main/CHANGELOG/CHANGELOG-2024.md)

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
