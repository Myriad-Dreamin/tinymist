# Change Log

All notable changes to the "tinymist" extension will be documented in this file.

Check [Keep a Changelog](http://keepachangelog.com/) for recommendations on how to structure this file.

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
