#let scan-backwards(s) = {
  let len = s.len() - 1
  let i = len
  while i > 0 {
    i -= 1
  }
  i
}

#let assign-from-unknown(unknown) = {
  let i = 0
  while unknown > 0 {
    i = unknown - 1
    unknown -= 1
  }
  i
}
