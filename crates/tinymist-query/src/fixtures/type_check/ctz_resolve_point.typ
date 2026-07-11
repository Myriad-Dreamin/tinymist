#let ctz-get-points(ctx) = {
  ctx.shared-state.at("ctz-points", default: (:))
}

#let ctz-resolve-point(ctx, p) = {
  if type(p) == str {
    let points = ctz-get-points(ctx)
    if p in points {
      return points.at(p)
    }
    let (_, point) = coordinate.resolve(ctx, p)
    return point
  }
  panic("Cannot resolve point")
}

#let ctz-resolve-vector(ctx, vector) = {
  let v1 = ctz-resolve-point(ctx, vector.at(0))
  let v2 = ctz-resolve-point(ctx, vector.at(1))
  (v2.at(0) - v1.at(0), v2.at(1) - v1.at(1), 0)
}
