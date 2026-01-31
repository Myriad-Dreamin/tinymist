/// path: img.png
not-a-real-png

-----
/// path: data.csv
a,b,c

-----
/// contains: +img.png, -data.csv
#let p = ""/* range -1..0 */
#image(p)

