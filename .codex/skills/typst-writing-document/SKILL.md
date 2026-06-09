---
name: typst-writing-document
description: Use when authoring or validating Typst documents from canonical grammar examples, especially when you need compile, HTML, or SVG-based validation workflows.
metadata:
  short-description: Author Typst docs from canonical grammar examples
  from-tutorial-rev: https://github.com/typst-doc-cn/tutorial/tree/971815a6b4898c0339b620e08c62824ec03bba7f
---

# Writing and Validating Typst Documents

Use this skill when the user wants help drafting, fixing, or validating Typst
documents. Everything needed for grammar lookup lives in this file so it can be
copied into another repository without sibling scripts or reference files.

## Workflow

1. Start with the grammar lookup section in this file and pick the closest
   existing pattern before inventing new syntax.
2. Copy the smallest matching example, then adapt it incrementally.
3. Run `typst compile` after each meaningful edit. Any non-zero exit code is a
   blocking failure.
4. After compile succeeds, use HTML output to inspect rendered text and
   document structure when wording or content ordering matters.
5. Use SVG output plus Playwright MCP when you need to inspect visual layout,
   spacing, numbering, line breaks, or emphasis.

## Validation

Compile validation:

```sh
typst compile --root . path/to/document.typ target/typst-grammar-authoring-check/document.pdf
```

Text validation through HTML:

```sh
typst compile --root . --features html path/to/document.typ target/typst-grammar-authoring-check/document.html
rg "expected text" target/typst-grammar-authoring-check/document.html
```

Visual validation through SVG:

```sh
typst compile --root . path/to/document.typ target/typst-grammar-authoring-check/document.svg
typst compile --root . path/to/document.typ target/typst-grammar-authoring-check/document-{0p}.svg
```

Playwright inspection:

- Use Playwright only after SVG generation succeeds.
- Open the SVG directly if the MCP server supports local files.
- Otherwise use a tiny local HTML wrapper that embeds the SVG, then capture a
  screenshot and inspect layout, spacing, numbering, line breaks, and emphasis.

## Guardrails

- Keep all skill-authored prose and instructions in English.
- Canonical syntax examples may retain non-English literals from their source
  examples.
- Do not treat Tinymist or editor diagnostics as the source of truth.
- Do not assume sibling updater scripts, reference files, or repo-local
  metadata exist when using this skill elsewhere.
- Use `{p}` or `{0p}` in multi-page SVG output paths.
- Treat HTML export as a validation aid, not a production contract.
- Keep command examples platform-neutral by using forward-slash or placeholder
  paths.

## Grammar Lookup

This section is embedded on purpose so the skill stays self-contained. Examples
are derived from the unofficial tutorial grammar samples and kept compact so the
lookup remains usable in a single file.

<!-- BEGIN GENERATED GRAMMAR LOOKUP -->

### Base Elements

- `paragraph`: `writing-markup`
- `heading`: `= Heading`; `== Heading`
- `strong`: `*Strong*`
- `emph`: `_emphasis_`; `*_emphasis_*`
- `list`:

```typ
+ List 1
+ List 2
```

- `continue-list`:

```typ
4. List 1
+ List 2
```

- `emum`:

```typ
- Enum 1
- Enum 2
```

- `mix-list-emum`:

```typ
- Enum 1
  + Item 1
- Enum 2
```

- `raw`:

```typ
`code`
```

- `long-raw`:

````typ
``` code```
````

- `lang-raw`:

````typ
```rs  trait World```
````

- `blocky-raw`:

````typ
```typ
= Heading
```
````

- `image`: `#image("/assets/files/香風とうふ店.jpg", width: 50pt)`
- `image-stretch`: `#image("/assets/files/香風とうふ店.jpg", width: 50pt, height: 50pt, fit: "stretch")`
- `image-inline`: `在一段话中插入一个#box(baseline: 0.15em, image("/assets/files/info-icon.svg", width: 1em))图片。`
- `figure`:

````typ
#figure(```typ
#image("/assets/files/香風とうふ店.jpg")
```, caption: [用于加载香風とうふ店送外卖的宝贵影像的代码])
````

- `link`: `#link("https://zh.wikipedia.org")[维基百科]`
- `http-link`: `https://zh.wikipedia.org`
- `internal-link`:

```typ
== 某个标题 <ref-internal-link>
#link(<ref-internal-link>)[链接到某个标题]
```

- `table`: `#table(columns: 2, [111], [2], [3])`
- `table-align`: `#table(columns: 2, align: center, [111], [2], [3])`
- `inline-math`: `$sum_x$`
- `display-math`: `$ sum_x $`
- `escape-sequences`: `>\_<`
- `unicode-escape-sequences`: `\u{9999}`
- `newline-by-space`: `A \ B`
- `newline`:

```typ
A \
B
```

- `shorthand`: `北京--上海`
- `shorthand-space`: `A~B`
- `inline-comment`: `// 行内注释`
- `cross-line-comment`:

```typ
/* 行间注释
  */
```

- `box`: `在一段话中插入一个#box(baseline: 0.15em, image("/assets/files/info-icon.svg", width: 1em))图片。`

### Text Styling

- `highlight`: `#highlight[高亮一段内容]`
- `underline`: `#underline[Language]`
- `underline-evade`:

```typ
#underline(
  evade: false)[ጿኈቼዽ]
```

- `overline`: `#overline[ጿኈቼዽ]`
- `strike`: `#strike[ጿኈቼዽ]`
- `subscript`: `威严满满#sub[抱头蹲防]`
- `superscript`: `香風とうふ店#super[TM]`
- `text-size`: `#text(size: 24pt)[一斤鸭梨]`
- `text-fill`: `#text(fill: blue)[蓝色鸭梨]`
- `text-font`: `#text(font: "Microsoft YaHei")[板正鸭梨]`

### Script Declarations

- `enter-script`: `#1`
- `code-block`: `#{"a"; "b"}`
- `content-block`: `#[内容块]`
- `none-literal`: `#none`
- `false-literal`: `#false`
- `true-literal`: `#true`
- `integer-literal`: `#(-1), #(0), #(1)`
- `n-adecimal-literal`: `#(-0xdeadbeef), #(-0o644), #(-0b1001)`
- `float-literal`: `#(0.001), #(.1), #(2.)`
- `exp-repr-float`: `#(1e2), #(1.926e3), #(-1e-3)`
- `string-literal`: `#"Hello world!!"`
- `str-escape-sequences`: `#"\""`
- `str-unicode-escape-sequences`: `#"\u{9999}"`
- `array-literal`: `#(1, "OvO", [一段内容])`
- `dict-literal`: `#(neko-mimi: 2, "utterance": "喵喵喵")`
- `empty-array`: `#()`
- `empty-dict`: `#(:)`
- `paren-empty-array`: `#(())`
- `single-member-array`: `#(1,)`
- `var-decl`: `#let x = 1`
- `func-decl`: `#let f(x) = x * 2`
- `closure`: `#let f = (x, y) => x + y`
- `named-param`: `#let g(named: none) = named`
- `variadic-param`: `#let g(..args) = args.pos().join([、])`
- `destruct-array`: `#let (one, hello-world) = (1, "Hello, World")`
- `destruct-array-eliminate`: `#let (_, second, ..) = (1, "Hello, World", []); #second`
- `destruct-dict`: `#let (neko-mimi: mimi) = (neko-mimi: 2); #mimi`
- `array-remapping`:

```typ
#let (a, b, c) = (1, 2, 3)
#let (b, c, a) = (a, b, c)
#a, #b, #c
```

- `array-swap`:

```typ
#let (a, b) = (1, 2)
#((a, b) = (b, a))
#a, #b
```

- `placeholder`:

```typ
#let last-two(t) = {
  let _ = t.pop()
  t.pop()
}
#last-two((1, 2, 3, 4))
```

### Script Statements

- `if`:

```typ
#if true { 1 },
#if false { 1 } else { 0 }
```

- `if-if`:

```typ
#if false { 0 } else if true { 1 },
#if false { 2 } else if false { 1 } else { 0 }
```

- `while`:

```typ
#{
  let i = 0;
  while i < 10 {
    (i * 2, )
    i += 1;
  }
}
```

- `for`:

```typ
#for i in range(10) {
  (i * 2, )
}
```

- `for-destruct`: `#for (特色, 这个) in (neko-mimi: 2) [猫猫的 #特色 是 #这个\ ]`
- `break`: `#for i in range(10) { (i, ); (i + 1926, ); break }`
- `continue`:

```typ
#for i in range(10) {
  if calc.even(i) { continue }
  (i, )
}
```

- `return`:

```typ
#let never(..args) = return
#type(never(1, 2))
```

- `include`: `#include "other-file.typ"`

### Script Styling

- `set`:

```typ
#set text(size: 24pt)
四斤鸭梨
```

- `scope`:

```typ
两只#[兔#set text(fill: rgb("#ffd1dc").darken(15%))
  #[兔白#set text(fill: orange)
  又白]，真可爱
]
```

- `set-if`:

```typ
#let is-dark-theme = true
#set rect(fill: black) if is-dark-theme
#set text(fill: white) if is-dark-theme
#rect([wink!])
```

- `show-set`:

```typ
#show: set text(fill: blue)
wink!
```

- `show`:

````typ
#show raw: it => it.lines.at(1)
获取代码片段第二行内容：```typ
#{
set text(fill: true)
}
```
````

- `text-selector`:

```typ
#show "cpp": strong(emph(box("C++")))
在古代，cpp是一门常用语言。
```

- `regex-selector`:

```typ
#show regex("[”。]+"): it => {
  set text(font: "KaiTi")
  highlight(it, fill: yellow)
}
“无名，万物之始也；有名，万物之母也。”
```

- `label-selector`:

```typ
#show <一整段话>: set text(fill: blue)
#[$lambda$语言是世界上最好的语言。] <一整段话>

另一段话。
```

- `selector-exp`:

```typ
#show heading.where(level: 2): set text(fill: blue)
= 一级标题
== 二级标题
```

- `here`: `#context here().position()`
- `here-calc`: `#context [ 页码是偶数：#calc.even(here().page()) ]`
- `query`: `#context query(<ref-internal-link>).at(0).body`
- `state`: `#state("my-state", 1)`

### Script Expressions

- `func-call`: `#calc.pow(4, 3)`
- `content-param`: `#emph[emphasis]`
- `member-exp`:

```typ
#`OvO`.text
```

- `method-exp`: `#"Hello World".split(" ")`
- `dict-member-exp`:

```typ
#let cat = (neko-mimi: 2)
#cat.neko-mimi
```

- `content-member-exp`:

```typ
#`OvO`.text
```

- `repr`: `#repr[ 一段文本 ]`
- `type`: `#type[一段文本]`
- `eval`: `#type(eval("1"))`
- `eval-markup-mode`: `#eval("== 一个标题", mode: "markup")`
- `array-in`:

```typ
#let pol = (1, "OvO", [])
#(1 in pol)
```

- `array-not-in`:

```typ
#let pol = (1, "OvO", [])
#([另一段内容] not in pol)
```

- `dict-in`:

```typ
#let cat = (neko-mimi: 2)
#("neko-mimi" in cat)
```

- `logical-cmp-exp`:

```typ
#(1 < 0), #(1 >= 2),
#(1 == 2), #(1 != 2)
```

- `logical-calc-exp`: `#(not false), #(false or true), #(true and false)`
- `plus-exp`: `#(+1), #(+0), #(1), #(++1)`
- `minus-exp`:

```typ
#(-1), #(-0), #(--1),
#(-+-1)
```

- `arith-exp`:

```typ
#(1 + 1), #(1 + -1),
#(1 - 1), #(1 - -1)
```

- `assign-exp`: `#let a = 1; #repr(a = 10), #a, #repr(a += 2), #a`
- `string-concat-exp`: `#("a" + "b")`
- `string-mul-exp`: `#("a" * 4), #(4 * "ab")`
- `string-cmp-exp`: `#("a" == "b"), #("a" != "b"), #("a" < "ab"), #("a" >= "a")`
- `int-to-float`: `#float(1), #(type(float(1)))`
- `bool-to-int`: `#int(true), #(type(int(true)))`
- `float-to-int`: `#int(1), #(type(int(1)))`
- `dec-str-to-int`: `#int("1"), #(type(int("1")))`
- `nadec-str-to-int`:

```typ
#let safe-to-int(x) = {
  let res = eval(x)
  assert(type(res) == int, message: "should be integer")
  res
}
#safe-to-int("0xf"), #(type(safe-to-int("0xf"))) \
#safe-to-int("0o755"), #(type(safe-to-int("0o755"))) \
#safe-to-int("0b1011"), #(type(safe-to-int("0b1011"))) \
```

- `num-to-str`:

```typ
#repr(str(1)),
#repr(str(.5))
```

- `int-to-nadec-str`: `#str(501, base:16), #str(0xdeadbeef, base:36)`
- `bool-to-str`: `#repr(false)`
- `int-to-bool`:

```typ
#let to-bool(x) = x != 0
#repr(to-bool(0)),
#repr(to-bool(1))
```

<!-- END GENERATED GRAMMAR LOOKUP -->
