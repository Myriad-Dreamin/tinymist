#let done(value) = (kind: "done", value: value)

#let forward(handler, value) = {
  if value == 0 {
    done(value)
  } else {
    handler(value - 1)
  }
}

#let dispatch(value) = {
  if value == 0 {
    done(value)
  } else if value == 1 {
    forward(dispatch, value)
  } else {
    forward(dispatch, value - 1)
  }
}

#let use-zero = dispatch(0)
#let use-one = dispatch(1)
