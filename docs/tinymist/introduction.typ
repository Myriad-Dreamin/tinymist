// dist/tinymist/rs
#import "mod.typ": *

#show: book-page.with(title: "Introduction")

Tinymist [ˈtaɪni mɪst] is an integrated language service for #link("https://typst.app/")[Typst] [taɪpst]. You can also call it "微霭" [wēi ǎi] in Chinese.

It contains:
- an analyzing library for Typst, see #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/crates/tinymist-query")[tinymist-query].
- a CLI for Typst, see #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/crates/tinymist/")[tinymist].
  - which provides a language server for Typst, see #cross-link("/feature/language.typ")[Language Features].
  - which provides a preview server for Typst, see #cross-link("/feature/preview.typ")[Preview Feature].
- a VSCode extension for Typst, see #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/vscode/")[Tinymist VSCode Extension].

== Features

#include "feature/language-content.typ"

== Versioning and Release Cycle

Tinymist's versions follow the #link("https://semver.org/")[Semantic Versioning] scheme, in format of `MAJOR.MINOR.PATCH`. Besides, tinymist follows special rules for the version number:
- If a version is suffixed with `-rcN` ($N > 0$), e.g. `0.11.0-rc1` and `0.12.1-rc1`, it means this version is a release candidate. It is used to test publish script and E2E functionalities. These versions will not be published to the marketplace.
- If the `PATCH` number is odd, e.g. `0.11.1` and `0.12.3`, it means this version is a nightly release. The nightly release will use both #link("https://github.com/Myriad-Dreamin/tinymist/tree/main")[tinymist] and #link("https://github.com/typst/typst/tree/main")[typst] at *main branch*. They will be published as prerelease version to the marketplace.
- Otherwise, if the `PATCH` number is even, e.g. `0.11.0` and `0.12.2`, it means this version is a regular release. The regular release will always use the recent stable version of tinymist and typst.

The release cycle is as follows:
- If there is a typst version update, a new major or minor version will be released intermediately. This means tinymist will always align the minor version with typst.
- If there is at least a bug or feature added this week, a new patch version will be released.

== Installation

Follow the instructions to enable tinymist in your favorite editor.
- #cross-link("/frontend/vscode.typ")[VS Cod(e,ium)]
- #cross-link("/frontend/neovim.typ")[NeoVim]
- #cross-link("/frontend/emacs.typ")[Emacs]
- #cross-link("/frontend/sublime-text.typ")[Sublime Text]
- #cross-link("/frontend/helix.typ")[Helix]
- #cross-link("/frontend/zed.typ")[Zed]

== Installing Regular/Nightly Prebuilds from GitHub

Note: if you are not knowing what is a regular/nightly release, please don't follow this section.

Besides published releases specific for each editors, you can also download the latest regular/nightly prebuilts from GitHub and install them manually.

- Regular prebuilts can be found in #link("https://github.com/Myriad-Dreamin/tinymist/releases")[GitHub Releases].
- Nightly prebuilts can be found in #link("https://github.com/Myriad-Dreamin/tinymist/actions")[GitHub Actions]. For example, if you are seeking a nightly release for the featured #link("https://github.com/Myriad-Dreamin/tinymist/pull/468")[PR: build: bump version to 0.11.17-rc1], you could click and go to the #link("https://github.com/Myriad-Dreamin/tinymist/actions/runs/10120639466")[action page] run for the related commits and download the artifacts.

To install extension file (the file with `.vsix` extension) manually, please #kbd("Ctrl+Shift+X") in the editor window and drop the downloaded vsix file into the opened extensions view.

== Documentation

See #link("https://myriad-dreamin.github.io/tinymist/")[Online Documentation].

== Packaging

Stable Channel:

#link(
  "https://repology.org/project/tinymist/versions",
  md-alter(
    "Packaging status",
    () => image("https://repology.org/badge/vertical-allrepos/tinymist.svg", alt: "Packaging status"),
  ),
)

Nightly Channel:

#link(
  "https://repology.org/project/tinymist-nightly/versions",
  md-alter(
    "Packaging status",
    () => image("https://repology.org/badge/vertical-allrepos/tinymist-nightly.svg", alt: "Packaging status"),
  ),
)

== Roadmap

The development in typst v0.12.0 has been finished. We'll slow down for a while to catch regressions and bugs by changes. We are also planning to implement the following features in typst v0.13.0 or spare time in weekend:

- Spell checking: There is already a branch but no suitable (default) spell checking library is found.
- Type checking: complete the type checker.
- Static Linter: linting code statically according to feedback of the type checker and succeeding code analysis.
- Periscope renderer: It is disabled since vscode reject to render SVGs containing foreignObjects.
- Inlay hint: It is disabled _by default_ because of performance issues.
- Find references of dictionary fields and named function arguments.
- A reliable ways of configuring projects's entry files and files to export across editors. See #link("https://github.com/Myriad-Dreamin/tinymist/issues/530")[GitHub Issue 530.]
- Improve symbol view's appearance.
- Improve package view.
  - Navigate to symbols by clicking on the symbol name in the view.
  - Automatically locate the symbol item in the view when viewing local documentation.
  - Remember the recently invoked package commands, e.g. "Open Docs of \@preview/cetz:0.3.1", "Open directory of \@preview/touying:0.5.3".
- Improve label view.
  - Group labels.
  - Search labels.
  - Keep (persist) group preferences.
- Improve Typst Preview.
  - Browsing mode: if no main file is specified, the preview will be in browsing mode and use the recently focused file as the main.
  - Pin drop-down: Set the file to preview in the drop-down for clients that doesn't support passing arguments to the preview command.
  - Render in web worker (another thread) to reduce overhead on the electron's main thread.

If you are interested by any above features, please feel free to send Issues to discuss or PRs to implement to #link("https://github.com/Myriad-Dreamin/tinymist")[GitHub.]

== Contributing

Please read the #link("CONTRIBUTING.md")[CONTRIBUTING.md] file for contribution guidelines.

== Maintainers

Get list of maintainers from #link("https://github.com/Myriad-Dreamin/tinymist/blob/main/MAINTAINERS.typ")[MAINTAINERS.typ]. Or programmatically by `yarn maintainers`

#note-box[
  You can add extra arguments for specific information. For example, `yarn maintainers --input="action=maintainers"`.
]

== Acknowledgements

- Partially code is inherited from #link("https://github.com/nvarner/typst-lsp")[typst-lsp]
- The #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/vscode#symbol-view")[integrating] *offline* handwritten-stroke recognizer is powered by #link("https://detypify.quarticcat.com/")[Detypify].
- The #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/vscode#preview-command")[integrating] preview service is powered by #link("https://github.com/Enter-tainer/typst-preview")[typst-preview].
- The #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/vscode#managing-local-packages")[integrating] local package management functions are adopted from #link("https://github.com/OrangeX4/vscode-typst-sync")[vscode-typst-sync].
