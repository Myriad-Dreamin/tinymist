
#import "typings.typ": *

/// Renders a latex equation.
#let mimath(body: pos(str)) = content;

/// A mitex module for rendering latex equations.
#let mitex = interface(mimath: mimath);

/// Renders a markdown string.
///
/// - body (str): The markdown content to render.
/// - mitex (module): The module to render latex equation.
///
/// -> content
#let render(body: pos(str), mitex: named(mitex)) = content;
