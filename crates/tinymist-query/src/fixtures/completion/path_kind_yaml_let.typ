/// path: data.yml
a: 1

-----
/// path: data.yaml
a: 1

-----
/// path: data.json
{"a": 1}

-----
/// contains: +data.yml, +data.yaml, -data.json
#let p = ""/* range -1..0 */
#yaml(p)

