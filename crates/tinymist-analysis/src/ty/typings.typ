
#let typing-item(kind: none, ..args) = (func: typing-item, kind: kind, args: args);

#let func = typing-item.with(kind: "func");
#let var(..args) = typing-item.with(kind: "var", ..args);
#let tv(..args) = typing-item.with(kind: "tv", ..args);
#let arr(..args) = typing-item.with(kind: "arr", ..args);
#let tuple(..args) = typing-item.with(kind: "tuple", ..args);
