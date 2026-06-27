/// path: typst.toml
[package]
name = "fixture-anonly-minstack-noapply"
version = "0.1.0"
entrypoint = "src/analyze.typ"

-----
/// path: src/stackframe.typ
#let pause(fun) = (..args) => (..env) => cont => (fun: fun, args: args)
#let tailcall(fun) = (..args) => (fun: fun, args: args)

-----
/// path: src/match.typ
#import "stackframe.typ"

#let commit() = (subparse, input) => (ok: true, backtrack: false, next: none, rest: input)
#let eof() = (subparse, input) => (ok: true, backtrack: true, next: none, rest: input)
#let regex(arg: auto) = (subparse, input) => (ok: true, backtrack: true, val: input, next: none, rest: input)
#let seq(pats: auto, array: true) = (subparse, input) => stackframe.pause(subparse)(pats.at(0, default: ()), input)()(ans => ans)
#let str(arg: auto) = (subparse, input) => (ok: true, backtrack: true, val: arg, next: none, rest: input)
#let iter(pat: auto) = (subparse, input) => stackframe.pause(subparse)(pat, input)()(ans => ans)
#let star(pat: auto) = (subparse, input) => stackframe.pause(subparse)(pat, input)()(ans => ans)
#let fork(pats: auto) = (subparse, input) => stackframe.pause(subparse)(pats.at(0, default: ()), input)()(ans => ans)
#let maybe(pat: auto) = (subparse, input) => (ok: true, backtrack: true, val: (), next: none, rest: input)
#let try(pat: auto) = (subparse, input) => stackframe.pause(subparse)(pat, input)()(ans => (: ..ans, backtrack: true))
#let peek(pat: auto) = (subparse, input) => (ok: true, backtrack: true, next: none, rest: input)
#let neg(pat: auto) = (subparse, input) => (ok: true, backtrack: true, next: none, rest: input)
#let error(msg: auto, pat: auto) = (subparse, input) => (ok: false, backtrack: true, msg: msg, next: none, rest: input)
#let hint(len: auto, mapping: auto) = (subparse, input) => stackframe.tailcall(subparse)(mapping.at("__", default: ()), input)

-----
/// path: src/analyze.typ
#import "match.typ"

#let reachable(grammar) = {
  let explore(pat) = {
    if "lab" in pat {
      (pat.lab,)
    } else if "pat" in pat {
      explore(pat.pat)
    } else if "pats" in pat {
      for sub in pat.pats { explore(sub) }
    } else {
      ()
    }
  }
  let reach = (:)
  for (id, rule) in grammar {
    reach.insert(id, explore(rule.pat).dedup())
  }
  reach
}

#let reachable-closure(grammar, start) = {
  let _ = reachable(grammar)
  start
}

#let inv-reachable-closure(grammar, start) = {
  let _ = reachable(grammar)
  start
}

#let check-closed(grammar) = (
  undef: (),
  dangling: (),
  dangerous: inv-reachable-closure(grammar, ()),
)

#let check-empty(grammar) = {
  let _and(l, r) = if l == false or r == false { false } else if l == true { r } else if r == true { l } else { none }
  let _or(l, r) = if l == true or r == true { true } else if l == false { r } else if r == false { l } else { none }
  let /* ident after */ known-nonempty(pat, known) = {
    if pat == () {
      none
    } else if "lab" in pat {
      known.at(pat.lab, default: none)
    } else if pat.call == match.regex {
      true
    } else if pat.call in (match.star, match.commit, match.maybe, match.peek, match.neg, match.eof) {
      false
    } else if pat.call == match.str {
      pat.arg != ""
    } else if pat.call == match.fork {
      let ans = true
      for sub in pat.pats { ans = _and(ans, known-nonempty(sub, known)) }
      ans
    } else if pat.call == match.hint {
      let ans = true
      for (_, sub) in pat.mapping { ans = _and(ans, known-nonempty(sub, known)) }
      ans
    } else if pat.call == match.seq {
      let ans = false
      for sub in pat.pats { ans = _or(ans, known-nonempty(sub, known)) }
      ans
    } else if pat.call in (match.error,) {
      true
    } else if pat.call in (match.iter, match.try) {
      known-nonempty(pat.pat, known)
    } else {
      panic(pat)
    }
  }
  let emps = (:)
  for (id, rule) in grammar {
    if emps.at(id, default: none) == none {
      emps.insert(id, known-nonempty(rule.pat, emps))
    }
  }
  emps
}

#let check-leftrec(grammar, nonempty) = {
  let next-left(pat) = {
    if pat == () {
      ((), true)
    } else if "lab" in pat {
      ((pat.lab,), nonempty.at(pat.lab, default: none) != true)
    } else if pat.call == match.regex {
      ((), false)
    } else if pat.call == match.str {
      ((), pat.arg == "")
    } else if pat.call == match.fork {
      let ans = ()
      let after = true
      for sub in pat.pats {
        let (also, go-on) = next-left(sub)
        ans += also
        after = after or go-on
      }
      (ans, after)
    } else if pat.call == match.hint {
      let ans = ()
      let after = true
      for (_, sub) in pat.mapping {
        let (also, go-on) = next-left(sub)
        ans += also
        after = after or go-on
      }
      (ans, after)
    } else if pat.call == match.seq {
      let ans = ()
      for sub in pat.pats {
        let (also, go-on) = next-left(sub)
        ans += also
        if not go-on { return (ans, false) }
      }
      (ans, true)
    } else if pat.call in (match.commit,) {
      ((), true)
    } else if pat.call in (match.maybe, match.peek, match.neg, match.star) {
      let (ans, _) = next-left(pat.pat)
      (ans, true)
    } else if pat.call in (match.eof,) {
      ((), false)
    } else if pat.call in (match.iter, match.try, match.error) {
      next-left(pat.pat)
    } else {
      panic(pat)
    }
  }
  let cycles = ()
  for (id, rule) in grammar {
    let (nl, _) = next-left(rule.pat)
    if nl != () { cycles.push((id, ..nl)) }
  }
  (
    cycles: cycles,
    dangerous: inv-reachable-closure(grammar, cycles.flatten()),
  )
}
