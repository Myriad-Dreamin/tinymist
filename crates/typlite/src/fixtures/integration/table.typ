#table(
  columns: (1em,) * 20,
  ..range(20).map(x => [#x]),
)

#table(
  columns: (1fr, 2fr) * 3,
  align: (left, center, right) * 2,
  ..range(20).map(x => [#x]),
)

// merge cell
#table(
  columns: (1fr, 2fr) * 3,
  ..range(20).map(x => [#x]),
  ..range(20).map(x => table.cell(colspan: 2)[#x]),
)

// fallback
#table(
  columns: 2,
  [Header], [Row],
  ```
  Code line 1
  Code line 2
  ```,
  [Regular text],
)
