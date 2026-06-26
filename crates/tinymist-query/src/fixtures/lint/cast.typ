

#let _convert-mat(ctx, rec, inner) = {
  let nrows = inner.rows.len()
  let ncols = if inner.rows.len() > 0 {
    inner.rows.first().len()
  } else {
    0
  }
  let aug = inner.augment
  if type(aug) == int {
    if aug >= ncols {
      return _err(
        ctx,
        "cannot draw a vertical line after column " + str(aug) + " of a matrix with " + str(ncols) + " columns",
        inner,
      )
    }
    if aug >= nrows {
      return _err(
        ctx,
        "cannot draw a horizontal line after row " + str(aug) + " of a matrix with " + str(nrows) + " rows",
        inner,
      )
    }
  } else if type(aug) == dictionary {} else {
    return _err(ctx, "expected either a int or a dictionary for augment but got", aug, inner)
  }
}
