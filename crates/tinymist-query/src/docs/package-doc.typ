#import "@preview/cmarker:0.1.6": render
// re-export page template
#import "/typ/templates/page.typ": project, is-md-target
#let book-page = project

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
#let markdown-docs = render.with(
  scope: (
    image: (src, alt: none) => {
      html.elem("img", attrs: (src: src, alt: alt, class: "code-image"))
    },
  ),
)

#let symbol-doc(in-module: none, info) = {
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
    par(html.elem("a", attrs: (href: info.symbol_link, class: "symbol-link"), "Symbol Docs"))
  }
  //     let convert_err = None::<EcoString>;
  if info.parsed_docs != none {
    if info.parsed_docs.kind == "func" {
      //     if let Some(DefDocs::Function(sig)) = &info.parsed_docs {
      //         // let _ = writeln!(out, "<!-- begin:sig -->");
      //         let _ = writeln!(out, "```typc");
      //         let _ = write!(out, "let {}", info.name);
      //         let _ = sig.print(&mut out);
      //         let _ = writeln!(out, ";");
      //         let _ = writeln!(out, "```");
      //         // let _ = writeln!(out, "<!-- end:sig -->");
      //     }
      // repr(info.parsed_docs)
    }
  }

  let printed_docs = false
  if not info.is_external {
    //     let convert_err = None::<EcoString>;
    if info.parsed_docs != none {
      let docs = info.parsed_docs
      if docs.docs != none and docs.docs.len() > 0 {
        // remove_list_annotations(docs.docs())
        printed_docs = true
        markdown-docs(docs.docs)
      }
      if docs.kind == "func" {
        //                 for param in docs
        //                     .pos
        //                     .iter()
        //                     .chain(docs.named.values())
        //                     .chain(docs.rest.as_ref())
        //                 {
        //                     // let _ = writeln!(out, "<!-- begin:param {} -->", param.name);
        //                     let ty = match &param.cano_type {
        //                         Some((short, _, _)) => short,
        //                         None => "unknown",
        //                     };
        //                     let title = format!("{} ({ty:?})", param.name);
        //                     let _ = writeln!(
        //                         out,
        //                         "#labelled-heading(4, {title:?})\n\n#markdown-docs({:?})\n",
        //                         param.docs
        //                     );
        //                     // let _ = writeln!(out, "<!-- end:param -->");
        //                 }
      }
    }
  }

  if not printed_docs {
    let plain_docs = info.docs
    if plain_docs == none {
      plain_docs = info.oneliner
    }
    // todo: eval with error tolerance?
    // if plain_docs != none {
    //   eval(plain_docs, mode: "markup")
    // }

    //     if let Some(lnk) = &child.module_link {
    //         match lnk.as_str() {
    //             "builtin" => {
    //                 let _ = writeln!(out, "A Builtin Module");
    //             }
    //             _lnk => {
    //                 // let _ = writeln!(out, "#link({})[Module Docs]\n",
    //                 // TypstLink(lnk));
    //             }
    //         }
    //     }

    //     // let _ = writeln!(out, "<!-- end:symbol {ident} -->");
    //     let _ = writeln!(out, "]),");
  }
}
#let module-doc(info: none, name: none, m) = {
  if info.prefix.len() > 0 {
    let primary = info.prefix
    let title = "Module: " + primary + " in " + info.prefix

    module-divider
    labelled-heading(2, title)
  }

  for child in m.children {
    symbol-doc(in-module: info.prefix, child)
  }
}
#let package-doc(info) = {
  let info = json(info)
  let title = "@" + info.meta.namespace + "/" + info.meta.name + " " + info.meta.version

  show: book-page.with(title: title)
  html.elem("style", read("/typ/packages/package-docs/global.css"))
  show: html.elem.with("main")

  html.elem("h1", attrs: (id: "package-doc-title"), title)

  let repo = info.meta.manifest.package.at("repository", default: none)
  if repo != none {
    let repo_link = html.elem("a", attrs: (href: repo, class: "package-repo-link"), "Repository")
    html.elem("p", repo_link)
  }

  let description = info.meta.manifest.package.at("description", default: none)
  if description != none {
    description
  }

  for (name, m, info) in info.modules {
    module-doc(info: info, name: name, m)
  }
}
