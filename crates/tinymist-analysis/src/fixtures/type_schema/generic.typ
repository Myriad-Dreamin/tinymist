
#import "typings.typ": *

#let T = tv("T");
#let U = tv("U");

/// Maps the elements of the array to a new array using the given function.
///
/// - self (array): The array to map.
/// - f (function): The function to apply to each element.
/// -> array
#let map(self: pos(arr(T)), f: pos((elem: pos(T)) => U)) = arr(U);
