// dist/tinymist/rs
#import "mod.typ": *

#show: book-page.with(title: "Introduction")

Tinymist [ˈtaɪni mɪst] is an integrated language service for #link("https://typst.app/")[Typst] [taɪpst]. You can also call it "微霭" [wēi ǎi] in Chinese.

It contains:
- an analyzing library for Typst, see #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/crates/tinymist-query")[tinymist-query].
- a CLI for Typst, see #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/crates/tinymist/")[tinymist].
  - which provides a language server for Typst, see #cross-link("/language-features.typ")[Langauge Features].
  - which provides a preview server for Typst, see #cross-link("/preview-feature.typ")[Preview Feature].
- a VSCode extension for Typst, see #link("https://github.com/Myriad-Dreamin/tinymist/tree/main/editors/vscode/")[Tinymist VSCode Extension].

Further Reading:

- #cross-link("/overview.typ")[Overview of Service]
- #cross-link("/rs/tinymist/index.typ")[Tinymist Crate Docs (for Developers)]
