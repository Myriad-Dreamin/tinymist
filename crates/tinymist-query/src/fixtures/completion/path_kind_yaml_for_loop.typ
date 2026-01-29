/// path: a.yaml
a: 1

-----
/// path: b.yaml
b: 1

-----
/// contains: +a.yaml, +b.yaml
#let files = (""/* range -1..0 */,)
#for f in files {
  yaml(f)
}

