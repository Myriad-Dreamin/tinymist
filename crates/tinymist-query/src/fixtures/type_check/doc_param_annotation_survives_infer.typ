/// - name (string): Hook name.
#let hook(name) = {
  (type: "hook", name: name)
}

#let hook-value = hook("h")
