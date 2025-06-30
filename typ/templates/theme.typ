#import "@preview/shiroa:0.2.3": templates, book-sys
#import templates: *
#import "target.typ": sys-is-html-target, is-md-target

// Theme (Colors)
#let themes = theme-box-styles-from(toml("theme-style.toml"), xml: it => xml(it))
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
