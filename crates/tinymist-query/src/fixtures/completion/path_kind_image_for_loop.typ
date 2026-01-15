/// path: a.png
not-a-real-png

-----
/// path: b.png
not-a-real-png

-----
/// path: c.csv
a,b

-----
/// contains: +a.png, +b.png, -c.csv
#let files = (""/* range -1..0 */, "b.png")
#for file in files {
  image(file)
}

