# Change Log

All notable changes to the "tinymist" extension will be documented in this file.

Check [Keep a Changelog](http://keepachangelog.com/) for recommendations on how to structure this file.

## v0.10.3 - [2024-03-16]

### Commands/Tools (New)

* feat: support rest code lens in https://github.com/Myriad-Dreamin/tinymist/pull/45
  * Preview
  * Preview in ..
    * `doc` or `slide` mode
    * `tab` or `browser` target
  * Export as ..
    * PDF format
* feat: add init template command in https://github.com/Myriad-Dreamin/tinymist/pull/50
* feat: add template gallery as template picker in https://github.com/Myriad-Dreamin/tinymist/pull/52

### References (New)

* feat: support find/goto syntactic references in https://github.com/Myriad-Dreamin/tinymist/pull/34 and https://github.com/Myriad-Dreamin/tinymist/pull/42

### Autocompletion

* feat: upgrade compiler for autocompleting package in https://github.com/Myriad-Dreamin/tinymist/pull/30

### Definition

* dev: reimplements definition analysis in https://github.com/Myriad-Dreamin/tinymist/pull/43

### Inlay Hint

* feat: implement inlay hint configuration in https://github.com/Myriad-Dreamin/tinymist/pull/37
* feat: disable inlay hints on one line content blocks in https://github.com/Myriad-Dreamin/tinymist/pull/48
* dev: change position of inlay hint params in https://github.com/Myriad-Dreamin/tinymist/pull/51

### Misc

* feat: supports vscode variables in configurations, more testing, and validation in https://github.com/Myriad-Dreamin/tinymist/pull/53
  * You can set root/server/font path(s) with vscode variables. The variables are listed in https://www.npmjs.com/package/vscode-variables.

### Internal Optimization

* feat: deferred root resolution in https://github.com/Myriad-Dreamin/tinymist/pull/32
* feat: allow fuzzy selection to deref targets in https://github.com/Myriad-Dreamin/tinymist/pull/46
* feat: implements def-use analysis in https://github.com/Myriad-Dreamin/tinymist/pull/17, https://github.com/Myriad-Dreamin/tinymist/pull/19, https://github.com/Myriad-Dreamin/tinymist/pull/25, and https://github.com/Myriad-Dreamin/tinymist/pull/26

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.10.2...v0.10.3

## v0.10.2 - [2024-03-12]

* use implicit autocomplete in https://github.com/Myriad-Dreamin/tinymist/pull/3
* add the new context keyword in https://github.com/Myriad-Dreamin/tinymist/pull/6
* correctly drop sender after the server shutting down in https://github.com/Myriad-Dreamin/tinymist/pull/7
* feat: support more foldable AST nodes in https://github.com/Myriad-Dreamin/tinymist/pull/11

**Full Changelog**: https://github.com/Myriad-Dreamin/tinymist/compare/v0.10.1...v0.10.2

## v0.10.1 - [2024-03-11]

Initial release corresponding to Typst v0.11.0-rc1.
