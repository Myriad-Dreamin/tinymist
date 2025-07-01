#import "@preview/shiroa:0.2.3": book-sys, templates
#import templates: *
#import "target.typ": is-md-target, sys-is-html-target

/// Reads a theme from a preset dictionary and returns a structured theme object.
///
/// - preset (dictionary): A dictionary containing theme presets.
/// - xml (function): (Deprecated, passing `read` intead) A function to parse XML data.
/// - read (function): A function to read theme files.
/// - target (string): The target platform or style, such as "web-light", "web-dark", or "pdf".
/// -> dictionary
#let book-theme-from(preset, xml: xml, read: none, target: target) = {
  // todo: move theme style parser to another lib file
  let theme-target = if target.contains("-") {
    target.split("-").at(1)
  } else {
    "light"
  }
  let theme-style = preset.at(theme-target)

  let is-dark-theme = theme-style.at("color-scheme") == "dark"
  let is-light-theme = not is-dark-theme

  let main-color = rgb(theme-style.at("main-color"))
  let dash-color = rgb(theme-style.at("dash-color"))

  let code-theme-file = theme-style.at("code-theme")

  let code-extra-colors = if code-theme-file.len() > 0 {
    if read != none {
      theme-style.insert("code-theme", bytes(read(code-theme-file)))
    }
    let data = xml(theme-style.at("code-theme")).first()

    let find-child(elem, tag) = {
      elem.children.find(e => "tag" in e and e.tag == tag)
    }

    let find-kv(elem, key, tag) = {
      let idx = elem.children.position(e => "tag" in e and e.tag == "key" and e.children.first() == key)
      elem.children.slice(idx).find(e => "tag" in e and e.tag == tag)
    }

    let plist-dict = find-child(data, "dict")
    let plist-array = find-child(plist-dict, "array")
    let theme-setting = find-child(plist-array, "dict")
    let theme-setting-items = find-kv(theme-setting, "settings", "dict")
    let background-setting = find-kv(theme-setting-items, "background", "string")
    let foreground-setting = find-kv(theme-setting-items, "foreground", "string")
    (bg: rgb(background-setting.children.first()), fg: rgb(foreground-setting.children.first()))
  } else {
    (bg: rgb(239, 241, 243), fg: none)
  }


  (
    style: theme-style,
    is-dark: is-dark-theme,
    is-light: is-light-theme,
    main-color: main-color,
    dash-color: dash-color,
    code-extra-colors: code-extra-colors,
  )
}

/// Reads themes from a preset dictionary and returns structured theme objects.
///
/// - preset (dictionary): A dictionary containing theme presets.
/// - read (function): A function to read theme files.
/// - target (string): The target platform or style, such as "web-light", "web-dark", or "pdf".
/// -> dictionary
#let theme-box-styles-from(
  preset,
  read: read,
  target: target,
  light-theme: none,
  dark-theme: none,
) = {
  let sys-is-html-target = ("target" in dictionary(std))

  if light-theme == none {
    for (name, it) in preset.pairs() {
      if it.at("color-scheme") == "light" {
        light-theme = name
      }
    }
  }
  if dark-theme == none {
    for (name, it) in preset.pairs() {
      if it.at("color-scheme") == "dark" and dark-theme != "ayu" {
        dark-theme = name
      }
    }
  }

  if light-theme == none {
    light-theme = "light"
  }
  if dark-theme == none {
    dark-theme = "dark"
  }

  let book-theme-from = book-theme-from.with(xml: xml, read: read)

  // Theme (Colors)
  let dark-theme = book-theme-from(preset, target: "web-" + dark-theme)
  let light-theme = book-theme-from(preset, target: if sys-is-html-target { "web-" + light-theme } else { "pdf" })
  let default-theme = book-theme-from(preset, target: target)

  (
    dark-theme: dark-theme,
    light-theme: light-theme,
    default-theme: default-theme,
  )
}


#let theme-box(render, tag: "div", themes: none, class: none, theme-tag: none) = {
  let (
    dark-theme: dark-theme,
    light-theme: light-theme,
    default-theme: default-theme,
  ) = themes
  let is-md-target = target == "md"
  let sys-is-html-target = ("target" in dictionary(std))

  if is-md-target {
    show: html.elem.with(tag)
    show: html.elem.with("picture")
    html.elem("m1source", attrs: (media: "(prefers-color-scheme: dark)"), render(dark-theme))
    render(light-theme)
  } else if sys-is-html-target {
    if theme-tag == none {
      theme-tag = tag
    }
    html.elem(tag, attrs: (class: "code-image themed" + if class != none { " " + class }), {
      html.elem(theme-tag, render(dark-theme), attrs: (class: "dark"))
      html.elem(theme-tag, render(light-theme), attrs: (class: "light"))
    })
  } else {
    render(default-theme)
  }
}

// Theme (Colors)
#let themes = theme-box-styles-from(toml("theme-style.toml"), read: it => read(it))
#let (
  default-theme: (
    style: theme-style,
    is-dark: is-dark-theme,
    is-light: is-light-theme,
    main-color: main-color,
    dash-color: dash-color,
    code-extra-colors: code-extra-colors,
  ),
) = themes;
#let (
  default-theme: default-theme,
) = themes;
#let theme-box = theme-box.with(themes: themes)
