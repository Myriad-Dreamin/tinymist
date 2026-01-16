/// path: data.xml
<a>1</a>

-----
/// path: data.yaml
a: 1

-----
/// contains: +data.xml, -data.yaml
#let p = ""/* range -1..0 */
#xml(p)

