
#page({
  set text(size: 10.5pt)
  set block(spacing: 1.5em)

  show "TERMS AND CONDITIONS FOR USE": it => {
    v(0.5em)
    it
  }
  show "APPENDIX": it => {
    v(1fr)
    it
  }
  eval(read("/LICENSE"), mode: "markup")
  v(5em)
})
