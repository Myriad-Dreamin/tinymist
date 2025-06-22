
#import "target.typ": sys-is-html-target, is-md-target

#let git-head = read("/.git/HEAD").trim()
#let git-head-branch = if git-head.starts-with("ref: refs/heads/") {
  git-head.slice("ref: refs/heads/".len())
} else {
  none
}
#let git-head-hash = if git-head.starts-with("ref: ") {
  read("/.git/" + git-head.slice(5)).trim()
} else {
  git-head
}

// todo: read it from somewhere
#let remote = "https://github.com/Myriad-Dreamin/tinymist"

#let github-link(path, body, kind: "tree", permalink: true) = link(
  if is-md-target {
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
  },
  body,
)
