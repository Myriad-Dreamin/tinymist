#let fixed = (1, 2)
#let direct = (0, ..fixed, 3)

#let prepend(head, ..tail) = (head, ..tail)
#let prepended = prepend(0, 1, 2)

#let identity-args(..args) = args
#let captured = identity-args("x", "y")
#let respread = (0, ..captured, 3)
