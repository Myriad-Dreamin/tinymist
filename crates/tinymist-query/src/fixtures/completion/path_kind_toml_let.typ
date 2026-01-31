/// path: data.toml
a = 1

-----
/// path: data.yaml
a: 1

-----
/// contains: +data.toml, -data.yaml
#let p = ""/* range -1..0 */
#toml(p)

