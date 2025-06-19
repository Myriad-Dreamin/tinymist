
#let git-head = read("/.git/HEAD")
#let git-head-branch = if git-head.starts-with("ref: refs/heads/") {
  git-head.slice("ref: refs/heads/".len()).trim()
} else {
  none
}
#let git-head-hash = if git-head.starts-with("ref: ") {
  read("/.git/" + git-head.slice(5).trim()).trim()
} else {
  git-head.trim()
}

// todo: read it from somewhere
#let remote = "https://github.com/Myriad-Dreamin/tinymist"

#let github-link(path, body, kind: "tree", permalink: true) = link(
  {
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
