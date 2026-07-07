#import "/typ/packages/tinymist-index/lib.typ": create_index
#import "module.typ" as pages
#import "sidebar.typ" as layout
#import "source.typ" as sources

#let package-module-document = pages.package-module-document
#let package-module-symbol-document = pages.package-module-symbol-document
#let package-source-document = sources.package-source-document

#let package-doc(info, scip: none) = {
  let info = json(info)
  let index = if scip == none {
    none
  } else {
    create_index(scip)
  }
  let title = pages.package-title(info)

  pages.package-setup(title)

  let symbol-ctx = pages.symbol-context(info, index)

  layout.package-layout(info, symbol-ctx)[
    #pages.package-header(info, title)
    #for (name, m, info) in info.modules {
      pages.module-doc(info: info, name: name, symbol-ctx, m)
    }
  ]
}
