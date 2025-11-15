
/// path: resolve.typ
#let resolve(join, root, dir, name) = {
  let asset-dir = "assets"
  if sys.inputs.x-path-input-uri.ends-with(".png") {
    return (
      file: join(root, "images", sys.inputs.x-path-input-name),
      on-conflict: ```typc
      import "/resolve.typ": on-conflict; on-conflict(join, root, dir, name)
      ```.text,
    )
  }

  join(root, "assets", name)
};
-----
/// path: x_at_root.typ
