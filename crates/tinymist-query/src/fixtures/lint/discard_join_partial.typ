#let f() = {
  if true {
    [1]
  } else {
    [2]
    return;
  }
  return [];
}
