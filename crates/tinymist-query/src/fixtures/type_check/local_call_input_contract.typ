#let make-array(value) = range(value)
#let consume-array(values) = values
#let use-array(value) = consume-array(make-array(value))

#let consume-overwrite(values) = values
#let normalize(value: auto) = {
  value = make-array(2)
  consume-overwrite(value)
}

#let consume-before-overwrite(values) = {
  if values == none { none }
  values.at(0) = 2
  values = none
  values
}

#let overwrite-callee(value) = {
  if value == none { none }
  value = 1
  value
}

#let overwrite-tuple-callee(value) = {
  if value == none { none }
  value = 2
  value
}

#let seeded-tuple = overwrite-tuple-callee((1,))

#let call-before-refine() = {
  let value = "called"
  let result = overwrite-callee(value)
  overwrite-tuple-callee(value)
  result
}

#let result = use-array(2)
#let normalized = normalize()
#let consumed = consume-before-overwrite((1, 2))
#let refined = call-before-refine()
