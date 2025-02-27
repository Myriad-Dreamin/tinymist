
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
#let rgb2(hex: str) = func.with(alias: rgb);

/// Maps the elements of the array to a new array using the given function.
///
/// - self (array): The array to map.
/// - f (function): The function to apply to each element.
/// -> array
#let map(self: arr(T), f: (elem: T) => U) = func.with(ret: arr(U));
