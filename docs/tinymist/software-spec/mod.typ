#import "../mod.typ": *

#let translations = toml("/locales/tinymist-vscode.toml");

#let translate(desc) = {
  desc.replace(regex("\\%(.*?)\\%"), r => {
    let item = translations
    for p in r.captures.at(0).split(".") {
      item = item.at(p, default: none)
      if item == none {
        panic(`Missing translation for` + r.captures.at(0))
      }
    }
    item.en
  })
}
