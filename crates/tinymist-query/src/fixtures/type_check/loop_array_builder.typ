#let collect-pair() = {
  let result = ()
  for item in (1,) {
    result.push(item)
  }
  (none, ..result)
}

#let (_, collected) = collect-pair()

#let (_, direct-collected) = (none, ..(1,))

#let collect-rest(..items) = {
  let result = ()
  for item in items.pos() {
    result.push(item)
  }
  (none, ..result)
}

#let (_, rest-collected) = collect-rest("x")

#let positional-via-binding(..items) = {
  let positional = items.pos
  positional()
}

#let rebound-pos = positional-via-binding("a", 2)

/// -> array
#let collect-annotated-rest(..items) = {
  let result = ()
  for item in items.pos() {
    result.push(item)
  }
  (none, ..result)
}

#let (_, annotated-collected) = collect-annotated-rest("y")

#let collect-reassigned(..items) = {
  let result = ()
  for item in items.pos() {
    item = (1, 2, 0)
    result.push(item)
  }
  (none, ..result)
}

#let (_, reassigned-collected) = collect-reassigned("z")
