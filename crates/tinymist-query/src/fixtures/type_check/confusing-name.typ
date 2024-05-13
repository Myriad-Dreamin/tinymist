#let x(date) = date.display()

#let (x: x) = (x: 1)
#let master-cover(info, x: x) = {
  info = (submit-date: 0) + info
  x(datetime.today())
}

