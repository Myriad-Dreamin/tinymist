#import "mod.typ": *

#let package-sidebar(info, symbol-ctx, active-module-index: none, active-section: none, active-symbol-index: none) = {
  let package-link = if symbol-ctx.at("bundle", default: false) {
    let output = module-output-path(0, info.modules.at(0).at(2))
    if symbol-ctx.at("path", default: none) == output {
      module-anchor(info.modules.at(0).at(2))
    } else {
      relative-path(symbol-ctx.at("path", default: none), output)
    }
  } else {
    module-anchor(info.modules.at(0).at(2))
  }

  html.elem("aside", attrs: (class: "package-sidebar"), [
    #html.elem("nav", attrs: (class: "package-sidebar-nav", "aria-label": "Package modules"), [
      #html.elem("a", attrs: (class: "package-sidebar-title", href: package-link), [
        #html.elem("span", attrs: (class: "package-sidebar-package"), "@" + info.meta.namespace + "/" + info.meta.name)
        #html.elem("span", attrs: (class: "package-sidebar-version"), info.meta.version)
      ])
      #html.elem("div", attrs: (class: "package-sidebar-section"), "Modules")
      #html.elem("ul", attrs: (class: "package-sidebar-list"), [
        #let index = 0
        #for module-entry in info.modules {
          let module-info = module-entry.at(2)
          let module-def = module-entry.at(1)
          let class = "package-sidebar-link"
          if active-module-index == index {
            class = class + " is-active"
          }

          html.elem("li", attrs: (class: "package-sidebar-item"), [
            #html.elem(
              "a",
              attrs: (class: class, href: module-nav-dest(symbol-ctx, index, module-info)),
              module-nav-title(module-info),
            )
            #if active-module-index == index {
              html.elem("ul", attrs: (class: "package-sidebar-sublist"), [
                #for (section, title) in module-section-list {
                  let items = module-section-items(module-def, section)
                  if items.len() > 0 {
                    let section-class = "package-sidebar-sublink"
                    if active-section == section {
                      section-class = section-class + " is-active"
                    }
                    let section-symbol-ctx = (
                      in-module: module-info.prefix,
                      module-index: index,
                      module-info: module-info,
                      ..symbol-ctx,
                    )
                    html.elem("li", attrs: (class: "package-sidebar-subitem"), [
                      #html.elem("span", attrs: (class: section-class), title)
                      #html.elem("ul", attrs: (class: "package-sidebar-symbol-list"), [
                        #let item-index = 0
                        #for child in items {
                          let symbol-class = "package-sidebar-symbol-link"
                          if active-section == section and active-symbol-index == item-index {
                            symbol-class = symbol-class + " is-active"
                          }
                          html.elem("li", attrs: (class: "package-sidebar-symbol-item"), [
                            #html.elem(
                              "a",
                              attrs: (
                                class: symbol-class,
                                href: symbol-section-dest(section-symbol-ctx, child),
                              ),
                              sidebar-symbol-label(child),
                            )
                          ])
                          item-index += 1
                        }
                      ])
                    ])
                  }
                }
              ])
            }
          ])
          index += 1
        }
      ])
    ])
  ])
}

#let package-on-this-page() = {
  html.elem("aside", attrs: (class: "package-on-this-page", "data-ready": "false"), [
    #html.elem("nav", attrs: (class: "package-on-this-page-nav", "aria-label": "On this page"), [
      #html.elem("div", attrs: (class: "package-on-this-page-title"), "On this page")
      #html.elem("div", attrs: (class: "package-on-this-page-body"), [
        #html.elem("span", attrs: (class: "package-on-this-page-indicator", "aria-hidden": "true"))
        #html.elem("ul", attrs: (class: "package-on-this-page-list"))
      ])
    ])
    #html.elem("script", attrs: (type: "module"), read("on-this-page.js"))
  ])
}

#let package-layout(info, symbol-ctx, active-module-index: none, active-section: none, active-symbol-index: none, body) = {
  html.elem("div", attrs: (class: "package-doc-layout"), [
    #package-sidebar(
      info,
      symbol-ctx,
      active-module-index: active-module-index,
      active-section: active-section,
      active-symbol-index: active-symbol-index,
    )
    #html.elem("main", attrs: (class: "package-doc-main"), body)
    #package-on-this-page()
  ])
}
