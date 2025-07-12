/// path: jsx-typings.typ

/// Creates a counter component.
///
/// - initialCounter (int): The initial value for the counter.
/// -> content
#let Counter(initialCounter: 0) = none

-----
/// path: jsx-runtime.typ

#let typing-path = sys.inputs.at("x-jsx-typings", default: "jsx-typings.typ")

#import typing-path as typings

#let require = (it, hint: none) => (
  Counter: typings.Counter,
)

-----
/// contains: initialCounter
#import "jsx-runtime.typ": require

#let (Counter,) = require("$components/Counter.astro");

#(/* ident after */ Counter)
#Counter(/* range 0..1 */)
