
#let func(..args) = metadata((
  kind: "function",
  parmeters: extract-args(args),
));

#let var(..args) = metadata((
  kind: "variable",
));

#let elem(..args) = metadata((
  kind: "element",
  parmeters: extract-args(args),
));

#let arr(..args) = metadata(args);
#let union(..args) = metadata(args);
#let named(..args) = metadata(args);
#let type-of(..args) = metadata(args);
#let record(..args) = metadata(args);
#let refined(..args) = metadata(args);
