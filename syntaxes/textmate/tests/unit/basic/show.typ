#show: it => it
#show text: it => it
#show: rect
#show: list.item.with()
#show: rect.with(width: 1pt)
#show <_>: rect
#show "A": rect
#show regex("A"): (it) => it
#show raw.where(block: true): {
  rect()
}
#show { text }: (it) => it