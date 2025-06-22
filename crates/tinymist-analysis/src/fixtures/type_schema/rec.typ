
#import "typings.typ": *

#let V = tv("V");

#let dict-type(V: V) = {
  let Self = Self.with(V)
  rec(
    name: "dictionary",
    scope: (
      len: (self: Self) => int,
    ),
  )
};
