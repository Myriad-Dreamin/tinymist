/// path: fig.svg
<svg xmlns="http://www.w3.org/2000/svg" width="100" height="100" viewBox="0 0 100 100">
  <rect width="100" height="100" fill="red"/>
</svg>
-----
// Test image with bytes source by reading file
#let img_bytes = read("./fig.svg", encoding: none)
#image(img_bytes)

