// Issue #2444: parameter rename misses named argument labels at call sites.
// @typstyle off
#let bubble(/* ident after */ side: left, content) = {
  if side == left { [ok] }
  content
}

#bubble(side: left)[A]
#let bubble_left = bubble.with(side: left)
