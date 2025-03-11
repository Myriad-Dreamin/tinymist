
// A show rule, should be used like:
//   #show: super-plus-as-dagger
//   U^+U = U U^+ = I
// or in scope:
//   #[
//     #show: super-plus-as-dagger
//     U^+U = U U^+ = I
//   ]
#let super-plus-as-dagger(document) = {
  show math.attach: elem => {
    if __eligible(elem.base) and elem.at("t", default: none) == [+] {
      $attach(elem.base, t: dagger, b: elem.at("b", default: #none))$
    } else {
      elem
    }
  }

  document
}
