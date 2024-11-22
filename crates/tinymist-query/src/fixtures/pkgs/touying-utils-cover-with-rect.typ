/// path: lib.typ


/// Cover content with a rectangle of a specified color. If you set the fill to the background color of the page, you can use this to create a semi-transparent overlay.
///
/// Example: `#utils.cover-with-rect(fill: "red")[Hidden]`
///
/// - cover-args (args): The arguments to pass to the rectangle.
///
/// - fill (color): The color to fill the rectangle with.
///
/// - inline (boolean): Indicates whether the content should be displayed inline. Default is `true`.
///
/// - body (content): The content to cover.
#let cover-with-rect(..cover-args, fill: auto, inline: true, body) = {
  if fill == auto {
    panic("`auto` fill value is not supported until typst provides utilities to" + " retrieve the current page background")
  }
  if type(fill) == str {
    fill = rgb(fill)
  }

  let to-display = layout(layout-size => {
    context {
      let body-size = measure(body)
      let bounding-width = calc.min(body-size.width, layout-size.width)
      let wrapped-body-size = measure(box(body, width: bounding-width))
      let named = cover-args.named()
      if "width" not in named {
        named.insert("width", wrapped-body-size.width)
      }
      if "height" not in named {
        named.insert("height", wrapped-body-size.height)
      }
      if "outset" not in named {
        // This outset covers the tops of tall letters and the bottoms of letters with
        // descenders. Alternatively, we could use
        // `set text(top-edge: "bounds", bottom-edge: "bounds")` to get the same effect,
        // but this changes text alignment and also misaligns bullets in enums/lists.
        // In contrast, `outset` preserves spacing and alignment at the cost of adding
        // a slight, visible border when the covered object is right next to the edge
        // of a color change.
        named.insert("outset", (top: 0.15em, bottom: 0.25em))
      }
      stack(
        spacing: -wrapped-body-size.height,
        body,
        rect(fill: fill, ..named, ..cover-args.pos()),
      )
    }
  })
  if inline {
    box(to-display)
  } else {
    to-display
  }
}

-----
/// contains: body, fill, stroke
#import "lib.typ": *
#cover-with-rect(/* range 0..1 */)[];
