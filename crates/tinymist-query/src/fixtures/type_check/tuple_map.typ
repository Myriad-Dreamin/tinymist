#let a = (1,);
#let f = x => str(x);
#let b = a.map(f);
#let conditional = (if true { (1, 2) } else { (3, 4) }).map(f);
