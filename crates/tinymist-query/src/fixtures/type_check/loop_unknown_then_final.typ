#let loop-unknown-then-dict(..args) = {
  let out = (:)
  for (k, v) in args.named() {
    if v == auto {
      continue
    }
    out.insert(k, v)
    v
  }
  out
}

