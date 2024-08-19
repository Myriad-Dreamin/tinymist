// dist/tinymist/rs
#import "mod.typ": *

#show: book-page.with(title: "Introduction")

Tinymist [ˈtaɪni mɪst] is an integrated language service for #link("https://typst.app/")[Typst] [taɪpst]. You can also call it "微霭" [wēi ǎi] in Chinese.

It contains:
- an analyzing library for Typst, see #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/crates/tinymist-query")[tinymist-query].
- a CLI for Typst, see #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/crates/tinymist/")[tinymist].
  - which provides a language server for Typst, see #cross-link("/feature/language.typ")[Langauge Features].
  - which provides a preview server for Typst, see #cross-link("/feature/preview.typ")[Preview Feature].
- a VSCode extension for Typst, see #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/vscode/")[Tinymist VSCode Extension].

== Features

#include "feature/language-content.typ"

== Release Cycle

Tinymist follows the #link("https://semver.org/")[Semantic Versioning] scheme. The version number is in the format of `MAJOR.MINOR.PATCH`. The release cycle is as follows:
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

To install extension file (the file with `.vsix` extension) manually, please #kbd("Ctrl+Shift+X") in the editor window and drop the downloaded vsix file into the opended extensions view.

== Documentation

See #link("https://myriad-dreamin.github.io/tinymist/")[Online Documentation].

== Packaging

#link(
  "https://repology.org/project/tinymist/versions",
  md-alter(
    "Packaging status",
    () => image("https://repology.org/badge/vertical-allrepos/tinymist.svg", alt: "Packaging status"),
  ),
)

== Roadmap

After development for two months, most of the features are implemented. There are still some features to be implemented, but I would like to leave them in typst v0.12.0. I'll also pick some of them to implement on my weekends. Also please feel free to contribute if you are interested in the following features.

- Documentation and refactoring: It is my current focus.
- Spell checking: There is already a branch but no suitable (default) spell checking library is found.
- Periscope renderer: It is disabled since vscode reject to render SVGs containing foreignObjects.
- Inlay hint: It is disabled _by default_ because of performance issues.
- Find references of labels, dictionary fields, and named function arguments.
- Go to definition of dictionary fields and named function arguments.
- Autocompletion for raw language tags.
- Improve symbol view's appearance.

== Contributing

Please read the #link("CONTRIBUTING.md")[CONTRIBUTING.md] file for contribution guidelines.

== Acknowledgements

- Partially code is inherited from #link("https://github.com/nvarner/typst-lsp")[typst-lsp]
- The #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/vscode#symbol-view")[integrating] *offline* handwritten-stroke recognizer is powered by #link("https://detypify.quarticcat.com/")[Detypify].
- The #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/vscode#preview")[integrating] preview service is powered by #link("https://github.com/Enter-tainer/typst-preview")[typst-preview].
