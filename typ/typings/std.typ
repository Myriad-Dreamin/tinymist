
#import "/typ/packages/typings/lib.typ": *

#let U = tv("U");
#let V = tv("V");
#let W = tv("W");

#let dict-type(V: V) = {
  let Self = Self.with(V)
  rec(
    name: "dictionary",
    scope: (
      len: (self: Self) => int,
      at: (self: Self, key: pos(str), default: pos(V)) => V,
      insert: (self: Self, key: pos(str), value: pos(V)) => none,
      remove: (self: Self, key: pos(str), default: pos(V)) => none,
      keys: (self: Self) => arr(str),
      values: (self: Self) => arr(V),
      pairs: (self: Self) => arr(tuple(str, V)),
    ),
  )
};

#let array-type(V: V) = {
  let Arr = Self
  let Self = Self.with(V)
  rec(
    name: "array",
    scope: (
      range: (end: pos(int), start: named(int, 0), step: named(int, 1)) => arr(int),
      len: (self: Self) => int,
      first: (self: Self) => V,
      last: (self: Self) => V,
      at: (self: Self, index: pos(int), default: pos(V)) => V,
      push: (self: Self, value: pos(V)) => none,
      insert: (self: Self, index: pos(int), value: pos(V)) => none,
      remove: (self: Self, index: pos(str), default: pos(V)) => none,
      slice: (self: Self, start: pos(int), end: pos(int), count: pos(int)) => arr(V),
      contains: (self: Self, value: pos(V)) => bool,
      find: (self: Self, searcher: (elem: pos(V)) => bool) => V,
      position: (self: Self, searcher: (elem: pos(V)) => bool) => int,
      filter: (self: Self, searcher: (elem: pos(V)) => bool) => arr(V),
      map: (self: Self, f: pos((elem: pos(V)) => U)) => arr(U),
      enumerate: (self: Self, start: named(int, 0)) => arr(tuple(int, V)),
      zip: (self: Self, others: pos(arr(U)), exact: named(bool, false)) => arr(tuple(V, U)),
      fold: (
        self: Self,
        init: pos(U),
        folder: pos((acc: pos(U), elem: pos(V)) => U),
      ) => U,
      sum: (self: Self, default: pos(V)) => V,
      product: (self: Self, default: pos(V)) => V,
      any: (self: Self, test: (elem: pos(V)) => bool) => bool,
      all: (self: Self, test: (elem: pos(V)) => bool) => bool,
      flatten: (self: Arr.with(arr(V))) => arr(V),
      rev: (self: Self) => arr(V),
      split: (self: Self, at: pos(V)) => arr(arr(V)),
      join: sig(
        (self: Self, last: named(W)) => add(V, W),
        (self: Self, separator: pos(U), last: named(W)) => add(V, add(U, W)),
      ),
      intersperse: (self: Self, separator: pos(U)) => arr(union(V, U)),
      chunks: (self: Self, size: pos(int), exact: named(bool, false)) => arr(arr(V)),
      windows: (self: Self, window-size: pos(int)) => arr(arr(V)),
      sorted: (
        self: Self,
        key: named((elem: pos(V)) => any),
      ) => arr(V),
      dedup: (
        self: Self,
        key: named((elem: pos(V)) => any),
      ) => arr(V),
      to-dict: (
        self: Arr.with(tuple(str, V)),
      ) => dict(str, V),
      reduce: (
        self: Self,
        reducer: pos((acc: pos(V), elem: pos(V)) => V),
      ) => opt(V),
    ),
  )
};


#let assert = union(
  (condition: pos(bool), message: named(str)) => invariant(condition),
  eq: (left: pos(any), right: pos(any), message: named(str)) => invariant(eq(left, right)),
  ne: (left: pos(any), right: pos(any), message: named(str)) => invariant(neq(left, right)),
);
