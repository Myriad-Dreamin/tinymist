/// path: data.json
{"a": 1}

-----
/// path: data.yaml
a: 1

-----
/// contains: +data.json, -data.yaml
#let p = ""/* range -1..0 */
#json(p)

