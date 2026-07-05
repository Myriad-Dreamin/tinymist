#let l = (a) => (b) => l
#let r = (a) => (b) => (c) => r
#let merged = if true { l } else { r }
#let use = merged(1)
