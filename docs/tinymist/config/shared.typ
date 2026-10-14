
#import "mod.typ": *

#import "@preview/cmarker:0.1.6": render as md

#let is-vscode = state("config:is-vscode", false)
#let show-switch = state("config:show-switch", true)

#let translations = toml("/locales/tinymist-vscode.toml");

#let config = json("/editors/vscode/package.json").contributes.configuration.properties

#let other-config = json("/editors/vscode/package.other.json").contributes.configuration.properties

#context if show-switch.get() {
  if is-vscode.get() {
    switch-link("/config/vscode.typ", "/editors/vscode/Configuration.md")
  } else {
    switch-link("/config/neovim.typ", "/editors/neovim/Configuration.md")
  }
}

#let translate(desc) = {
  desc.replace(regex("\\%(.*?)\\%"), r => {
    let item = translations
    for p in r.captures.at(0).split(".") {
      item = item.at(p, default: none)
      if item == none {
        panic(`Missing translation for` + r.captures.at(0))
      }
    }
    item.en
  })
}

#let match-region(content, region-name) = {
  content.match(regex("// region " + region-name + "([\\s\\S]*?)// endregion " + region-name)).captures.at(0)
}

#let server-side-keys = (
  match-region(read("/crates/tinymist/src/config.rs"), "Configuration Items")
    .matches(regex(`"([^"]+)"`.text))
    .map(m => {
      "tinymist." + m.captures.at(0)
    })
)

#let is-server-side-config(key, is-other) = {
  if not is-vscode.get() and key.starts-with("tinymist.preview") and not is-other {
    return false
  }

  return server-side-keys.any(it => it == key or key.starts-with(it + "."))
};

#let config-type(t) = if type(t) == array {
  t.join[ | ]
} else {
  //     default:
  //       return "`unknown`";
  raw(t)
}

#let description-of(cfg) = if "markdownDescription" in cfg {
  md(translate(cfg.markdownDescription))
} else if "description" in cfg {
  translate(cfg.description)
} else {
  ""
}

#let config-shape(cfg, key) = {
  if "anyOf" in cfg {
    [This configuration item can be one of following types:]
    cfg
      .anyOf
      .map(it => list.item[
        #description-of(it)
        #config-shape(it, key)
      ])
      .join()
  }

  if "type" in cfg {
    list.item[*Type*: #config-type(cfg.type)]
  }

  // Enum Section
  if cfg.at("enum", default: none) != none [
    - *Valid Values*: #for (i, item) in cfg.enum.enumerate() [
        - #raw(lang: "json", { "\"" + item + "\"" })#if "enumDescriptions" in cfg [
            : #md(translate(cfg.enumDescriptions.at(i)))
          ]
      ]
  ]

  // Property Section
  if cfg.at("properties", default: none) != none [
    - *Properties*: #for (key, item) in cfg.properties.pairs() [
        - #raw(lang: "json", { "\"" + key + "\"" }):
          #description-of(item)
          #if type(item) == str { list.item[*Type*: #config-type(item)] } else { config-shape(item, key) }
      ]
  ]

  // Default Section
  // The language server disables compile-status notifications unless configured, so for the
  // server-side configuration docs the effective default is "disable"; the "enable" default
  // recorded in the VS Code package is an extension-level override.
  let default = if key == "tinymist.compileStatus" and not is-vscode.get() {
    "disable"
  } else {
    cfg.at("default", default: none)
  }
  if default != none {
    list.item[*Default*: #raw(lang: "json", json.encode(default))]
  }
}

#let config-item(key, cfg, is-other) = [
  #if "markdownDeprecationMessage" in cfg {
    return
  }
  #let is-vscode = is-vscode.get()

  #let prefix = is-vscode
  #let key-without-prefix = key.replace("tinymist.", "")
  #let key-with-prefix = "tinymist." + key-without-prefix
  #let name = if prefix { key-with-prefix } else { key-without-prefix }

  #if not is-vscode and not is-server-side-config(key-with-prefix, is-other) {
    return
  }

  #let description = description-of(cfg)

  = #raw(name)

  #description

  #if key == "tinymist.preview.background.enabled" and not is-vscode {
    // The background preview is created when the server starts, so this is an
    // initialization-time option: configuration updates sent later are not
    // picked up by the running server.
    [The server only reads this configuration at startup: it must be provided via `initializationOptions`, and changes sent afterwards (for example through `workspace/didChangeConfiguration`) have no effect.]
  }

  #config-shape(cfg, key)
]

#context {
  let is-vscode = is-vscode.get()

  let items = (
    config.pairs().filter(((k, _)) => k not in other-config).map(((key, cfg)) => (key, cfg, false))
      + other-config.pairs().map(((key, cfg)) => (key, cfg, true))
  )
  items = items.sorted(key: it => it.at(0))

  for (key, cfg, is-other) in items {
    config-item(key, cfg, is-other)
  }
}

