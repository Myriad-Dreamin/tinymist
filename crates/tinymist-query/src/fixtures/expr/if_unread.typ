#let if_unread() = {
  let x = none
  if true {
    x = 1
  } else {
    x = "s"
  }
  none
}
