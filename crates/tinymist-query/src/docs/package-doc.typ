#let module-divider = html.elem("hr", attrs: (class: "module-divider"));
#show link: it => if type(it.dest) == label {
  html.elem("a", attrs: (href: "#" + str(it.dest), class: "symbol-link"), it.body)
} else {
  it
}
#let heading-label(name) = {
  let it = name.replace(regex("[\s\:]"), "-").replace(regex("[.()]"), "").replace(regex("-+"), "-").replace("M", "m")
  label(it)
}
#let labelled-heading(depth, it) = {
  heading(depth: depth, html.elem("span", attrs: (id: str(heading-label(it))), it))
}
#let markdown-docs = raw.with(block: true, lang: "md");
#let module-doc(info: none, ..children) = {
  let info = json(bytes(info.text))

  if info.prefix.len() > 0 {
    let primary = info.prefix
    let title = "Module: " + primary + " in " + info.prefix

    module-divider
    labelled-heading(2, title)
  }

  children.pos().join()
}

#let symbol-doc(in-module: none, info: none, body) = {
  let info = json(bytes(info.text))
  // let ident = if !primary.is_empty() {
  //     eco_format!("symbol-{}-{primary}.{}", child.kind, child.name)
  // } else {
  //     eco_format!("symbol-{}-{}", child.kind, child.name)
  // };
  let title = {
    info.kind + ": " + info.name + " in " + in-module
  }

  labelled-heading(3, title)

  if info.symbol_link != none {
    // let _ = writeln!(out, "#link({})[Symbol Docs]\n", TypstLink(lnk));
    html.elem("a", attrs: (href: info.symbol_link, class: "symbol-link"), "Symbol Docs")
  }

  body
}
