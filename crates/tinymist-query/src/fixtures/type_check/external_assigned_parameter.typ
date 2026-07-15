/// path: base.typ
#let overwrite(value) = {
  value = 3
  value
}

-----
#import "base.typ": *

#let text-result = overwrite("ignored")
#let number-result = overwrite(42)
