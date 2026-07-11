#let choose(flag) = {
  if flag == "tuple" {
    return (1, 2)
  }
  if flag == "string" {
    return "ok"
  }
  panic("bad flag")
}

#let tuple = choose("tuple")
#let string = choose("string")
