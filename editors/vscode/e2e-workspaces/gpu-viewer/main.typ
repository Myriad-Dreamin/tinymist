
#set page(width: 12cm, height: 16cm, margin: 1cm, fill: black)
#set text(white)

= Tinymist GPU Viewer 1

This workspace configures Tinymist to load the GPU previewer provider extension.

#rect(
  width: 100%,
  inset: 12pt,
  radius: 4pt,
  stroke: blue,
)[
  The active previewer should resolve from
  `myriad-dreamin.tinymist-gpu-viewer`.
]

#grid(
  columns: (1fr, 1fr),
  gutter: 8pt,
  [#strong[Mode]\ Document preview], [#strong[Renderer]\ GPU viewer provider],
)
