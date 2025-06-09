#import "@preview/cmarker:0.1.6": render
// re-export page template
#import "/typ/templates/page.typ": project, is-md-target
#let book-page = project

#show: book-page.with(title: [Touying Docs])

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
  let info = json(bytes(info.text))

  let title = "@" + info.meta.namespace + "/" + info.meta.name + " " + info.meta.version

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

  // info
}
#{
  show raw: it => html.elem("style", it.text)
  // --vp-font-family-base: "Inter Variable", "Inter", "Noto Sans SC",
  //   "PingFang SC", "Microsoft Yahei", ui-sans-serif, system-ui, sans-serif,
  //   "Apple Color Emoji", "Segoe UI Emoji", "Segoe UI Symbol", "Noto Color Emoji";
  // font-family: var(--vp-font-family-base);
  ```css
    main {
      margin: 2em;
    }
    :root.light {
      --main-color: #000;
      --main-hover-color: #222939;
      --raw-bg-color: rgba(101, 117, 133, 0.16);
      --main-bg-color: #fafcfc;
      --nav-bg-color: #fafcfc;
      --gray-color: #6d6d6d;
      --accent: oklch(51.51% .2307 257.85);
      --accent-dark: oklch(64.94% .1982 251.813);
      --black: #0f1219;
    }

    :root {
      --main-color: #dfdfd6;
      --main-hover-color: #fff;
      --gray-color: #939da3;
      --raw-bg-color: #65758529;
      --main-bg-color: #212737;
      --nav-bg-color: #212737;
      --accent: oklch(71.7% .1648 250.794);
      --accent-dark: oklch(51.51% .2307 257.85);
      --vp-font-family-mono: ui-monospace, "Menlo", "Monaco", "Consolas",
        "Liberation Mono", "Courier New", monospace;
    }
    body {

      margin: 0;
      padding: 0;
      text-align: justify;
      background: var(--main-bg-color);
      background-size: 100% 600px;
      word-wrap: break-word;
      overflow-wrap: break-word;
      color: var(--main-color);
      line-height: 1.7;
    }

    h1 :target,
  h2 :target,
  h3 :target,
  h4 :target,
  h5 :target,
  h6 :target {
    scroll-margin-top: 1.25em;
  }
  h1 {
    font-size: 2.75em;
    margin-block-start: 0em;
    margin-block-end: 0.8888889em;
    margin-inline-start: 0px;
    margin-inline-end: 0px;
    line-height: 1.1111111;
  }
  h2 {
    font-size: 2em;
    margin-block-start: 1.6em;
    margin-block-end: 0.6em;
    margin-inline-start: 0px;
    margin-inline-end: 0px;
    line-height: 1.3333333;
  }
  h3 {
    font-size: 1.5em;
    margin-block-start: 1.5em;
    margin-block-end: 0.6em;
    margin-inline-start: 0px;
    margin-inline-end: 0px;
    line-height: 1.45;
  }
  h4 {
    font-size: 1.25em;
    margin-block-start: 1.5em;
    margin-block-end: 0.6em;
    margin-inline-start: 0px;
    margin-inline-end: 0px;
    line-height: 1.6;
  }
  h5 {
    font-size: 1.1em;
    margin-block-start: 1.5em;
    margin-block-end: 0.5em;
    margin-inline-start: 0px;
    margin-inline-end: 0px;
    line-height: 1.5;
  }
  p {
    margin-block-end: 0.5em;
  }
  strong,
  b {
    font-weight: 700;
  }
  a,
  .link {
    color: var(--accent);
    text-decoration: underline;
    cursor: pointer;
  }
  a,
  .link {
    transition: color 0.1s, underline 0.1s;
  }
  a:hover,
  .link:hover {
    color: var(--accent-dark);
    text-decoration: underline solid 2px;
  }
  textarea {
    width: 100%;
    font-size: 16px;
  }
  input {
    font-size: 16px;
  }
  table {
    width: 100%;
  }
  img {
    max-width: 100%;
    height: auto;
    border-radius: 8px;
  }
  pre,
  code,
  kbd,
  samp {
    font-family: var(--vp-font-family-mono);
  }
  code {
    padding: 2px 5px;
    background-color: var(--raw-bg-color);
    border-radius: 2px;
  }
  pre {
    padding: 1.5em;
    border-radius: 8px;
  }
  pre > code {
    all: unset;
  }
  blockquote {
    border-left: 4px solid var(--accent);
    padding: 0 0 0 18px;
    margin: 0px;
    font-size: 1.333em;
  }
  hr {
    border: none;
    border-top: 1px solid var(--raw-bg-color);
  }

  .sr-only {
    border: 0;
    padding: 0;
    margin: 0;
    position: absolute !important;
    height: 1px;
    width: 1px;
    overflow: hidden;
    /* IE6, IE7 - a 0 height clip, off to the bottom right of the visible 1px box */
    clip: rect(1px 1px 1px 1px);
    /* maybe deprecated but we need to support legacy browsers */
    clip: rect(1px, 1px, 1px, 1px);
    /* modern browsers, clip-path works inwards from each corner */
    clip-path: inset(50%);
    /* added line to stop words getting smushed together (as they go onto separate lines and some screen readers do not understand line feeds as a space */
    white-space: nowrap;
  }
  nav a,
  .social-links a {
    text-decoration: none;
    color: var(--main-color);
  }
  nav a:hover,
  .social-links a:hover {
    color: var(--main-hover-color);
  }
  .icon svg {
    width: 32px;
    height: 32px;
    overflow: visible;
  }
  .icon svg path,
  .icon svg circle {
    fill: currentColor;
  }
  .theme-icon {
    cursor: pointer;
  }
  .dark .theme-icon.light {
    display: none;
  }
  .dark .theme-icon.dark {
    display: dark;
  }
  .theme-icon.light {
    display: dark;
  }
  .theme-icon.dark {
    display: none;
  }
  .dark .code-image.themed .light {
    display: none;
  }
  .dark .code-image.themed .dark {
    display: initial;
  }
  .code-image.themed .light {
    display: initial;
  }
  .code-image.themed .dark {
    display: none;
  }

  figcaption {
    text-align: center;
  }
  .code-image svg {
    max-width: 100%;
    height: fit-content;
  }
  .inline-equation {
    display: inline-block;
    width: fit-content;
    margin: 0 0.15em;
  }
  .block-equation {
    display: grid;
    place-items: center;
    overflow-x: auto;
  }
  .block-list,
  .block-list li {
    margin: 0;
    padding: 0;
  }
  .block-list > li {
    list-style: none;
    margin-top: 1.5em;
    padding-left: 1em;
    border-left: 2.5px solid var(--main-color);
  }
  .block-list.accent > li {
    border-left: 2.5px solid var(--accent);
  }


  ```
}
#show: html.elem.with("main")
