#let array-push() = {
  let a = ()
  a.push(1)
}

#let dict-insert() = {
  let d = (:)
  d.insert("x", 1)
}

#let push-loop() = {
  let a = ()
  for i in range(2) {
    a.push(i)
  }
  a
}

#let push-dict-loop() = {
  let guides = ()
  for grp in ((members: (1,),),) {
    let g = (kind: "x")
    g.insert("width", 1)
    guides.push(g)
  }
  guides.sorted(key: g => g.kind)
}

#let push-dict-loop-if() = {
  let guides = ()
  for first in (true,) {
    let g = if first {
      let levels = ()
      if first { levels = levels.rev() }
      (kind: "swatch", levels: levels)
    } else {
      (kind: "other")
    }
    g.insert("width", 1)
    guides.push(g)
  }
  guides.sorted(key: g => g.kind)
}
