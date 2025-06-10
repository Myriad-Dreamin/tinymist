
#let typing-item(kind: none, ..args) = (func: typing-item, kind: kind, args: args);

#let sig(..args) = typing-item.with(kind: "sig", ..args);
#let var(..args) = typing-item.with(kind: "var", ..args);
#let tv(..args) = typing-item.with(kind: "tv", ..args);
#let arr(..args) = typing-item.with(kind: "arr", ..args);
#let tuple(..args) = typing-item.with(kind: "tuple", ..args);
#let pos(..args) = typing-item.with(kind: "pos", ..args);
#let named(..args) = typing-item.with(kind: "named", ..args);
#let rest(..args) = typing-item.with(kind: "rest", ..args);
#let prec(..args) = typing-item.with(kind: "prec", ..args);
#let succ(..args) = typing-item.with(kind: "succ", ..args);
#let union(..args) = typing-item.with(kind: "union", ..args);
#let intersect(..args) = typing-item.with(kind: "intersect", ..args);
#let interface(..args) = typing-item.with(kind: "interface", ..args);
#let satisfy = intersect;
