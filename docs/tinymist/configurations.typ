#import "mod.typ": *
#import "@preview/cmarker:0.1.0": render as md

#show: book-page.with(title: "Tinymist Configurations")

#let packages = json("/editors/vscode/package.json")

#let config-type(t) = if type(t) == array {
  raw(t.join(" | "))
} else {
  raw(t)
}

#let config_item(key, cfg) = [
  + *#raw(key)*:
    - Type: #config-type(cfg.type)
      #if cfg.type == "array" [
        - Items: #raw(cfg.items.type)
        - Description: #md(cfg.items.description)
      ]
    - Description: #md(cfg.description)
    #if cfg.at("enum", default: none) != none [
      - Valid values: #for (i, item) in cfg.enum.enumerate() [
            - #raw(item): #if "enumDescriptions" in cfg { md(cfg.enumDescriptions.at(i)) }
         ]
    ]
    #if type(cfg.default) == str {
      if cfg.default != "" [
        - Default: #raw(cfg.default)
      ] else [
        - Default: `""`
      ]
    } else if type(cfg.default) == array [
      - Default: [#cfg.default.join(",")]
    ] else [
      - Default: #cfg.default
    ]
]

#for (key, cfg) in packages.contributes.configuration.properties {
  config_item(key, cfg)
}

