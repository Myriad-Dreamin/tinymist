#import "mod.typ": *

#show: book-page.with(title: "Exporting to Other Markup Formats")

#note-box[
  This feature is currently in early development.
]

#github-link("/crates/typlite")[typlite] is a pure Rust library for converting Typst documents to other markup formats.


typlite's goal is to convert docstrings in typst packages to LSP docs (Markdown Format). To achieve this, it runs HTML export and extract semantic information from the HTML document for markup conversion.

#let pg-node = node.with(corner-radius: 2pt, shape: "rect");
#let out-format = box.with(width: 5em)
#let typlite-convert-graph = diagram(
  node-stroke: 1pt,
  edge-stroke: 1pt,
  pg-node((0.5, 0), [Typst Source Code]),
  edge("-|>", link("https://typst.app/docs/reference/html/")[HTML Export]),
  pg-node((3, 0), [```xml <xml:typlite/>```]),
  edge("-|>"),
  pg-node((5, 0), out-format[LaTeX]),
  edge((3, 0), (5, -0.7), "-|>"),
  pg-node((5, -0.7), out-format[Markdown]),
  edge((3, 0), (5, 0.7), "-|>"),
  pg-node((5, 0.7), out-format[DocX]),
)

#figure(
  cond-image(typlite-convert-graph),
  caption: [The conversion path from typst source code to other markup formats, by typlite.],
) <fig:typlite-conversion>

= TodoList

- [ ] Renders figures into PDF instead of SVG.
- [ ] Converts typst equations, might use #link("https://github.com/jgm/texmath")[texmath] or #link("https://codeberg.org/akida/mathyml")[mathyml] plus #link("https://github.com/davidcarlisle/web-xslt/tree/main/pmml2tex")[pmml2tex].

= Perfect Conversion

typlite is called "-ite" because it only ensures that nice docstrings are converted perfectly. Similarly, if your document looks nice, typlite can also convert it to other markup formats perfectly.

This introduces concept of _Semantic Typst._ To help conversion, you should separate styling scripts and semantic content in your typst documents.

A good example in HTML is ```html <strong>``` v.s. ```html <b>```. Written in typst,

```typ
#strong[Good Content]
#text(weight: 700)[Bad Content]
```

typlite can convert "Good Content" perfectly, but not "Bad Content". This is because we can attach markup-specific styles to "Good Content" then, but "Bad Content" may be broken by some reasons, such as failing to find font weight when rendering the content.

Let's show another example. We have a `main.typ`, which contains the abstract of our paper, and style it with the requirement of the journal. We write it like this:
#let mixed-content = ```typ
#align(center)[
  #text(weight: 700, size: 1.5em)[ABSTRACT]
  #text(size: 1.2em)[This is the abstract of my paper.]
]
```
#mixed-content
#eval(mixed-content.text, mode: "markup")

typlite has capability to convert the above content to other markup formats, but it feel cursed. This is because we mix the styling scripts and semantic content together. To separate them, a function `abstract` is can be created:

```typ
// template.typ
#let abstract(body) = align(center)[
  #text(weight: 700, size: 1.5em)[ABSTRACT]
  #text(size: 1.2em, body)
]
// main.typ
#import "/template.typ": abstract
#abstract[This is the abstract of my paper.]
```

The four function calls in the above example can well explain the difference between styling scripts and semantic content. calling `#abstract` from `main.typ` only provide "abstract material" and doesn't add any style, so it is a semantic content. while `#text(weight: 700)`, as a styling script, uses assigns styles to content and make difficult to understand the behind semantics.

typlite will feel happy and make perfect conversion if you keep aware of keep pure semantics of `main.typ` documents in the above way. In fact, this is probably also the way of people abstract typst templates from their documents.

= Example: Styling a Typst Document by IEEE LaTeX Template

#let paper-file-link(link, body) = github-link("/editors/vscode/e2e-workspaces/ieee-paper" + link, body)

The `main.typ` in the #paper-file-link("/")[Sample Workspace: IEEE Paper] can be converted perfectly.

- Run the command ```bash
  typlite main.typ main.tex --processor "/ieee-tex.typ"
  ```
- Create a project on Overleaf, using the #link("https://www.overleaf.com/latex/templates/ieee-demo-template-for-computer-society-conferences/hzzszpqfkqky")[IEEE LaTeX Template.]
- upload the `main.tex` file and exported PDF assets and it will get rendered and ready to submit.

= Example: Implementing `abstract` for IEEE LaTeX Template

Let's explain how #paper-file-link("/ieee-tex.typ")[`/ieee-tex.typ`] works by the `abstract` example. First, we edit #paper-file-link("/ieee-tex.typ")[`/ieee-template.typ`] to store the abstract in state:

```typ
#let abstract-state = state("tex:abstract", "")
#let abstract(body) = if is-html-target {
  abstract-state.update(_ => body)
} else {

}
```

#note-box[
  `is-html-target` already distinguishes regular typst PDF export and typlite export (which uses HTML export). We haven't decide a way to let your template aware of typlite export.

  Luckily, typst has `sys.input` mechanism so you can distinguish it by input by yourself:

  ```typ
  // typst compile or typlite main.typ --input x-target=typlite
  #let x-target = sys.inputs.at("x-target", default: "typst")
  #x-target // "typst" or "typlite"
  ```
]

Next, in the `ieee-tex.typ`, we can get the `abstract` material from the state and render it in the LaTeX template:

````typ
#let verbatim(body) = {
  show raw.where(lang: "tex"): it => html.elem("m1verbatim", attrs: (src: it.text))
  body
}

#let abstract = context {
  let abstract-body = state("tex:abstract", "").final()
  verbatim(```tex
  % As a general rule, do not put math, special symbols or citations
  % in the abstract
  \begin{abstract}
  ```)
  abstract-body
  verbatim(```tex
  \end{abstract}
  ```)
}
````

Currently, ```typc html.elem("m1verbatim")``` is the only `xml` element can be used by processor scripts. When seeing a `<m1verbatim/>` element, typlite writes the inner content to output directly.

#note-box[
  We don't extracts content `abstract-body` and wraps `verbatim` function. Inteadly, we put the body directly in the document, to let typlite process the equations in `abstract-body` and convert them to LaTeX.
]
