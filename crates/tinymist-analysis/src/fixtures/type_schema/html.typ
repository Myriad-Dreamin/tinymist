
#import "typings.typ": *

/// HTML element function
///
/// -> content
#let elem() = {
  let attr(..args) = named(interface(..args))

  /// Creates a `<svg/>` element.
  let svg(
    tag: pos("svg"),
    content: pos(content),
    attrs: attr(
      width: pos(str),
      height: pos(str),
      viewBox: pos(str),
      xmlns: pos(str),
    ),
  ) = content

  /// Creates a `<a/>` element
  let a(
    tag: pos("a"),
    content: pos(content),
    attrs: attr(
      href: pos(str),
      target: pos(str),
      rel: pos(str),
    ),
  ) = content

  sig(svg, a)
};
