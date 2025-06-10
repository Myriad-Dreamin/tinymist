
#let typing-item(kind: none, ..args) = (func: typing-item, kind: kind, args: args);

#let var(..args) = typing-item.with(kind: "var", ..args);
#let tv(..args) = typing-item.with(kind: "tv", ..args);
#let arr(..args) = typing-item.with(kind: "arr", ..args);
#let tuple(..args) = typing-item.with(kind: "tuple", ..args);
#let pos(..args) = typing-item.with(kind: "pos", ..args);
#let named(..args) = typing-item.with(kind: "named", ..args);
#let rest(..args) = typing-item.with(kind: "rest", ..args);
