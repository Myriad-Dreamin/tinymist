
#import "page.typ": plain-text

#let feature-state = state("feature-state")
#let features(content) = {
  feature-state.update(content)
  content
}

#let maintainer-state = state("maintainer-state")
#let maintainers(content) = {
  maintainer-state.update(content)
  content
}

#let embedded-meta(key, content) = metadata((
  kind: "embedded-meta",
  key: key,
  content: content,
))

#let description(content) = {
  embedded-meta("description", plain-text(content))
  [Description: ] + content
}

#let scope(..scopes) = {
  embedded-meta("scope", scopes.pos())
  [Scope: ]
  scopes.pos().map(raw).map(list.item).join()
}

#let github(content) = {
  let lnk = {
    "https://github.com/"
    content
  }

  embedded-meta("github-name", content)
  [Github: ]
  link(lnk, content)
}

#let email(content) = {

  // content

  [Email: ]
  let lnk = {
    "mailto:"
    content
  }

  embedded-meta("email", content)
  link(lnk, content)
}

#let maintains(content) = {

  let process-maintain-list(content) = {
    let is-item = it => it.func() == list.item
    content.children.filter(is-item).map(it => plain-text(it.body))
  }

  embedded-meta("maintains", process-maintain-list(content))
  [Maintains: ]
  content
}

#let cli-query(content) = {
  let action = sys.inputs.at("action", default: "")

  if action == "maintainers" {
    content.maintainers
  } else if action == "features" {
    content.features
  } else {
    content
  }
}

#let embed-query() = {
  let is-item = it => it.func() == list.item
  let feature-list = feature-state.final().children.filter(is-item)
  let maintainer-list = maintainer-state.final().children.filter(is-item)

  let collect-meta(content, res) = {
    if type(content) == array {
      for it in content {
        res = collect-meta(it, res)
      }
    } else if "body" in content.fields() {
      collect-meta(content.body, res)
    } else if "children" in content.fields() {
      collect-meta(content.children, res)
    } else if content.func() == metadata {
      let value = content.value
      if value.at("kind", default: "") == "embedded-meta" {
        res.insert(value.key, value.content)
      }
    }

    res
  }

  let features = feature-list.map(it => {
    let chs = it.body.children
    let p = chs.position(is-item)
    let name = chs.slice(0, p)
    let extras = collect-meta(chs.slice(p), (:))

    (
      name: plain-text(name.join()).trim(),
      ..extras,
    )
  })

  let maintainers = maintainer-list.map(it => {
    let chs = it.body.children
    let p = chs.position(is-item)
    let name = chs.slice(0, p)
    let extras = collect-meta(chs.slice(p), (:))

    (
      name: plain-text(name.join()).trim(),
      ..extras,
    )
  })

  [
    #let meta = (
      maintainers: maintainers,
      features: features,
    )
    #metadata(cli-query(meta)) <maintainer-meta>
  ]
}

#let main(content) = {
  context embed-query()
  show heading: it => {
    set text(2em)
    set block(above: 0.7em, below: 0.6em)
    it
  }
  content
}
