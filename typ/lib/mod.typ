
#import "typings.typ": *

#let T = var();
#let U = var();

/// Creates a new color from the given RGB components.
///
/// - r (int): The R component of the color.
/// - g (int): The G component of the color.
/// - b (int): The B component of the color.
/// -> color
#let rgb(r: int, g: int, b: int) = func;

/// Creates a new color from the given RGB components.
///
/// - hex (str): A hexadecimal representation of the color.
/// -> color
#let rgb2(hex: refined(str, regex("#[0-9a-fA-F]{0,6}"))) = func.with(alias: rgb);

/// Maps the elements of the array to a new array using the given function.
///
/// - self (array): The array to map.
/// - f (function): The function to apply to each element.
/// -> array
#let map(self: arr(T), f: (elem: T) => U) = func.with(ret: arr(U));

#let text(
  body: union(str, content),
  baseline: 0pt,
  costs: named((
    hyphenation: 100%,
    runt: 100%,
    widow: 100%,
    orphan: 100%,
  )),
) = elem;

#let csv(
  data: union(bytes, str),
  row-type: type-of(T),
) = func.with(ret: record(str, arr(T)));
