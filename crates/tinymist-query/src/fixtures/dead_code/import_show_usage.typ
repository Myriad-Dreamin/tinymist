/// path: widgets.typ

#let default-theme(body) = body
#let enum-style(body) = body

-----
/// path: main.typ
/// compile: true

#import "widgets.typ" as widgets
#import "widgets.typ": enum-style

#show: widgets.default-theme
#show: enum-style

content
