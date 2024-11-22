
/// -> content
#let _delayed-wrapper(body) = utils.label-it(
  metadata((kind: "touying-delayed-wrapper", body: body)),
  "touying-temporary-mark",
)

#(/* ident after */ _delayed-wrapper);
