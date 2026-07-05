#let via-method(ctx, layer) = {
  let mapping = (ctx.resolve-mapping)(layer)
  mapping
}

#let known-layer = (mapping: (x: "x"))
#let known-ctx = (
  resolve-mapping: layer => if layer == none { none } else { layer.mapping },
)
#let known-result = (known-ctx.resolve-mapping)(known-layer)

#let direct-known() = {
  let obj = (f: x => if x == none { none } else { x.mapping })
  obj.f((mapping: (y: "y")))
}
