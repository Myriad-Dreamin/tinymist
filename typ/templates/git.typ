
#import "target.typ": sys-is-html-target, is-md-target

#let git-head-input = sys.inputs.at("tinymist-git-head", default: none)
#let git-head-branch-input = sys.inputs.at("tinymist-git-head-branch", default: none)
#let git-head-hash-input = sys.inputs.at("tinymist-git-head-hash", default: none)

#let git-head = if git-head-input != none {
  git-head-input
} else {
  read("/.git/HEAD").trim()
}
#let git-head-branch = if git-head-branch-input != none and git-head-branch-input != "" {
  git-head-branch-input
} else if git-head.starts-with("ref: refs/heads/") {
  git-head.slice("ref: refs/heads/".len())
} else {
  none
}
#let git-head-hash = if git-head-hash-input != none and git-head-hash-input != "" {
  git-head-hash-input
} else if git-head.starts-with("ref: ") {
  read("/.git/" + git-head.slice(5)).trim()
} else {
  git-head
}

// todo: read it from somewhere
#let remote = "https://github.com/Myriad-Dreamin/tinymist"

#let github-link(path, body, kind: none, permalink: true) = {
  if kind == none {
    kind = if path.ends-with("/") { "tree" } else { "blob" }
  }

  let dest = if is-md-target {
    path
  } else {
    remote
    "/"
    kind
    "/"
    if not permalink or git-head-branch == none {
      git-head-branch
    } else {
      git-head-hash
    }
    "/"
    if path.starts-with("/") {
      path.slice(1)
    } else {
      path
    }
  }

  link(dest, body)
}
