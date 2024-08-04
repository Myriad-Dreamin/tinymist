
#let print-state = state("print-effect", ())
#let print(k, end: none) = print-state.update(it => it + (k, end))
#let println = print.with(end: "\n")

#let main = content => {
  context [
    #let prints = print-state.final()
    #metadata(prints.join()) <print-effect>
  ]
  content
}