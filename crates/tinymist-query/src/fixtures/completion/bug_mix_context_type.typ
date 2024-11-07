/// contains: base
#let tmpl2(x, y) = {
  assert(type(x) in (int, str) and type(y) == int)
  x + y
}
#tmpl2( /* range -1..0 */)