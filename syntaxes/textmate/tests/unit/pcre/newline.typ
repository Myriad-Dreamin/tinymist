
// Returns list of tuples, where the ith tuple contains:
#let _get-code-line-data(
  styles
) = {
  let line-spacing = 100pt

  for i in range(num-lines) {
    let indent-level = indent-levels.at(i)

    for j in range(calc.max(1, calc.ceil(line-width / real-text-width))) {
      let is-wrapped = j > 0
      let real-indent-level = if is-wrapped {0} else {indent-level}

      line-count += 1
    }

    line-data.push((line-wrapped-components, indent-level))
  }

  return line-data
}


// Create indent guides for a given line of a code element.
#let _code-indent-guides(
) = { style(styles => {
})}

