/// path: jsx-typings.typ

/// Creates a counter component.
///
/// - initialCounter (int): The initial value for the counter.
/// -> content
#let Counter(initialCounter: 0) = none

#let modules = (
  "$components/Counter.astro": (
    Counter: Counter,
  ),
)

-----
/// path: jsx-runtime.typ

#let typing-path = sys.inputs.at("x-jsx-typings", default: "jsx-typings.typ")

#import typing-path as typings

#let require = (it, hint: none) => typings.modules.at(it)

-----
#import "jsx-runtime.typ": require

#let (Counter: Counter) = require("$components/Counter.astro");

#(/* ident after */ Counter)
#Counter(initialCounter: 0)
