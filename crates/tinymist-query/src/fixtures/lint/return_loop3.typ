#let mul-mat(..matrices) = {
  matrices = matrices.pos()
  let out = matrices.remove(0)
  for matrix in matrices {
    out = for i in range(m) {
      (i,)
    }
  }
  return out
}
