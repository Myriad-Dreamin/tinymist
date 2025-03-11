#import "mod.typ": *
#import "@preview/cmarker:0.1.0": render as md

#show: book-page.with(title: [Configurations])

#let packages = json("/editors/vscode/package.json")

#let config-type(t) = if "anyOf" in t {
  let any-of = t.anyOf
  if type(any-of) == array {
    any-of.map(config-type).join(" | ")
  }
} else {
  if type(t.type) == array {
    t.type.join(" | ")
  } else {
    t.type
  }
}

#let config_item(key, cfg) = [
  + *#raw(key)*:
    - Type: #raw(config-type(cfg))
      #if "anyOf" in cfg {
        // todo: anyOf
      } else if cfg.type == "array" [
        - Items: #raw(cfg.items.type)
        - Description: #md(cfg.items.description)
      ]
    - Description: #md(cfg.at("markdownDescription", default: cfg.at("description", default: none)))
    #if cfg.at("enum", default: none) != none [
      - Valid values: #for (i, item) in cfg.enum.enumerate() [
            - #raw(item): #if "enumDescriptions" in cfg { md(cfg.enumDescriptions.at(i)) }
         ]
    ]
    #let cfg-default = cfg.at("default", default: none)
    #if type(cfg-default) == str {
      if cfg-default != "" [
        - Default: #raw(cfg-default)
      ] else [
        - Default: `""`
      ]
    } else if type(cfg-default) == array [
      - Default: [#cfg-default.join(",")]
    ] else if cfg-default != none [
      - Default: #cfg-default
    ]
]

#for (key, cfg) in packages.contributes.configuration.properties {
  config_item(key, cfg)
}

