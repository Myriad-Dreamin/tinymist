
#import "typings.typ": *

/// Creates a new rgb color
///
/// -> color
#let rgb() = {
  let comp = pos(union(float, ratio))

  /// Creates a new color from the given RGB components.
  ///
  /// - r (float, ratio): The R component of the color.
  /// - g (float, ratio): The G component of the color.
  /// - b (float, ratio): The B component of the color.
  /// -> color
  let rgb1(r: comp, g: comp, b: comp) = color

  /// Creates a new color from hexadecimal representation.
  ///
  /// - hex (str): A hexadecimal representation of the color.
  /// -> color
  let rgb2(hex: pos(str)) = color

  /// Creates a new color from other color.
  ///
  /// - other (color): The other color to convert.
  /// -> color
  let rgb3(other: pos(prec(color))) = color


  sig(rgb1, rgb2, rgb3)
};
