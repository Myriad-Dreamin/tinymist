/// path: data.csv
a,b,c

-----
/// path: img.png
not-a-real-png

-----
/// contains: +data.csv, -img.png
#let p = ""/* range -1..0 */
#csv(p)

