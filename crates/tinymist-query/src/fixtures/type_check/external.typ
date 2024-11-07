/// path: base.typ
#let bad-instantiate(x) = {
  let y; let z; let w;
  x + y + z + w
};
-----
#import "base.typ": *
#let prefix(title: none) = {
  bad-instantiate(title)
}