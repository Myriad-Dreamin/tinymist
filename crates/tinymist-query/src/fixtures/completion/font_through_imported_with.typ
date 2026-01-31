/// path: ori.typ
#let with(font: none) = it => {
  set text(font: font.main)
  it
}

-----
/// contains: "New Computer Modern"
#import "ori.typ"

#let fonts = (
  main: ""/* range -1..0 */,
  cjk: "Noto Serif CJK SC"
)

#show: ori.with(font: fonts)
