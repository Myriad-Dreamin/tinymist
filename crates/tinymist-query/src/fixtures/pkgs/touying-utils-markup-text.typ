/// path: lib.typ

/// Convert content to markup text, partly from
/// [typst-examples-book](https://sitandr.github.io/typst-examples-book/book/typstonomicon/extract_markup_text.html).
///
/// - it (content): The content to convert.
///
/// - mode (string): The mode of the markup text, either `typ` or `md`.
///
/// - indent (int): The number of spaces to indent. Default is `0`.
#let markup-text(it, mode: "typ", indent: 0) = {
  assert(mode == "typ" or mode == "md", message: "mode must be 'typ' or 'md'")
  let indent-markup-text = markup-text.with(mode: mode, indent: indent + 2)
  let markup-text = markup-text.with(mode: mode, indent: indent)
  if type(it) == str {
    it
  } else if type(it) == content {
    if it.func() == raw {
      if it.block {
        "\n" + indent * " " + "```" + it.lang + it
          .text
          .split("\n")
          .map(l => "\n" + indent * " " + l)
          .sum(default: "") + "\n" + indent * " " + "```"
      } else {
        "`" + it.text + "`"
      }
    } else if it == [ ] {
      " "
    } else if it.func() == enum.item {
      "\n" + indent * " " + "+ " + indent-markup-text(it.body)
    } else if it.func() == list.item {
      "\n" + indent * " " + "- " + indent-markup-text(it.body)
    } else if it.func() == terms.item {
      "\n" + indent * " " + "/ " + markup-text(it.term) + ": " + indent-markup-text(it.description)
    } else if it.func() == linebreak {
      "\n" + indent * " "
    } else if it.func() == parbreak {
      "\n\n" + indent * " "
    } else if it.func() == strong {
      if mode == "md" {
        "**" + markup-text(it.body) + "**"
      } else {
        "*" + markup-text(it.body) + "*"
      }
    } else if it.func() == emph {
      if mode == "md" {
        "*" + markup-text(it.body) + "*"
      } else {
        "_" + markup-text(it.body) + "_"
      }
    } else if it.func() == link and type(it.dest) == str {
      if mode == "md" {
        "[" + markup-text(it.body) + "](" + it.dest + ")"
      } else {
        "#link(\"" + it.dest + "\")[" + markup-text(it.body) + "]"
      }
    } else if it.func() == heading {
      if mode == "md" {
        it.depth * "#" + " " + markup-text(it.body) + "\n"
      } else {
        it.depth * "=" + " " + markup-text(it.body) + "\n"
      }
    } else if it.has("children") {
      it.children.map(markup-text).join()
    } else if it.has("body") {
      markup-text(it.body)
    } else if it.has("text") {
      if type(it.text) == str {
        it.text
      } else {
        markup-text(it.text)
      }
    } else if it.func() == smartquote {
      if it.double {
        "\""
      } else {
        "'"
      }
    } else {
      ""
    }
  } else {
    repr(it)
  }
}
-----
/// contains: typ
#import "lib.typ": *
#markup-text(mode: /* range 0..1 */)[];
-----
/// contains: 0
#import "lib.typ": *
#markup-text(indent: /* range 0..1 */)[];
