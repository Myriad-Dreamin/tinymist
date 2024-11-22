#let f(
  x,
  y,
  z,
  w01: 1,
  w02: "test",
  w03: 1 + 2,
  w04: <test>,
  w05: box,
  w06: list.item,
  w07: [..],
  w08: {
    1 + 2
  },
  w09: (1 + 2),
  w10: (1, 2),
  w11: (),
  w12: (:),
  w13: (a: 1),
  w14: (a: box),
  w15: (a: list.item),
) = 1

#(/* ident after */ f);
