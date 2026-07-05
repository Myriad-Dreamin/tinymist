
#let total-tests = state("total-tests", (0, 0))
#let test-set = state("test-set", (:))
#let example-set = state("example-set", (:))
#let ref-paths = state("ref-paths", (:))

#let reset() = {
  test-set.update(it => {
    for test in it.keys() {
      it.insert(test, "stale")
    }
    it
  })
  example-set.update(it => {
    for test in it.keys() {
      it.insert(test, "stale")
    }
    it
  })
}

#let running-tests(tests, examples) = {
  total-tests.update(it => (tests, examples))
}

#let running-test(test) = {
  test-set.update(it => {
    it.insert(test, "running")
    it
  })
}

#let passed-test(test) = {
  test-set.update(it => {
    it.insert(test, "passed")
    it
  })
}
#let failed-test(test) = {
  test-set.update(it => {
    it.insert(test, "failed")
    it
  })
}


#let running-example(example) = {
  example-set.update(it => {
    it.insert(example, "running")
    it
  })
}

#let passed-example(example) = {
  example-set.update(it => {
    if it.at(example) == "running" {
      it.insert(example, "passed")
    }
    it
  })
}

#let failed-example(example) = {
  example-set.update(it => {
    it.insert(example, "failed")
    it
  })
}

#let mismatch-example(example, hint) = {
  example-set.update(it => {
    it.insert(example, "failed")
    it
  })
  ref-paths.update(it => {
    it.insert(example, hint)
    it
  })
}

#let main(it) = {
  context {
    let tests = test-set.final()
    let examples = example-set.final()
    let ref-paths = ref-paths.final()
    let (total-tests, total-examples) = total-tests.final()

    [
      Running #total-tests tests, #total-examples examples.

    ]

    let tests = tests.pairs().sorted()
    for (test, status) in tests [
      #if status == "stale" {
        continue
      }

      + Test(#text(fill: blue.darken(10%), test)): #if status == "running" [
          #text(fill: yellow)[Running]
        ] else if status == "passed" [
          #text(fill: green.darken(30%))[Passed]
        ] else [
          #text(fill: red)[Failed]
        ]
    ]

    let examples = examples.pairs().sorted()
    for (example, status) in examples [
      #if status == "stale" {
        continue
      }

      + Example(#text(fill: blue.darken(10%), example)): #if status == "running" [
          #text(fill: yellow)[Running]
        ] else if status == "passed" [
          #text(fill: green.darken(30%))[Passed]
        ] else [
          #link(
            label("hint-" + example),
            text(fill: red)[Failed]
          )
        ]
    ]

    set page(height: auto)

    for (example, hint) in ref-paths.pairs() {
      page[
        == Hint #text(fill: blue.darken(10%), example) #label("hint-" + example)

        #text(fill: red)[compare image at #text(fill: blue.darken(10%), "/" + hint)]
        #grid(
          align: center,
          columns: (1fr, 1fr),
          [Ref], [Got],
          image("/" + hint), image("/" + hint.slice(0, hint.len() - 4) + ".tmp.png"),
        )
      ]
    }
  }

  it
}
