// Test destructuring with spread operator
#let data = (1, 2, 3, 4, 5)

#let (first, ..rest) = data
#let (a, ..unused_rest) = data
#let (b, ..) = data

#first
#rest
#a
#b
// rest is used, unused_rest should be warned
