
#import "/typ/packages/typings/lib.typ": *

#let U = tv("U");
#let V = tv("V");
#let W = tv("W");

// array generic
// :: arguments.at(default)
// -> arguments.at

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
      filter: (self: Self, test: (elem: pos(V)) => bool) => arr(V),
      map: (self: Self, mapper: pos((elem: pos(V)) => U)) => arr(U),
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

// bytes
// calc

// datetime
// decimal

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

// duration
// eval
// float
// function
// int
// label
// dictionary
// none
// panic
// plugin
// regex
// repr
// selector
// string
// symbol
// system
// target
// type
// version

#let str-type = rec(
  name: "str",
  scope: (
    clusters: (self: Self) => arr(str),
    codepoints: (self: Self) => arr(int),
    // todo: match object
    match: (self: Self, pattern: pos(union(str, regex))) => dictionary,
    matches: (self: Self, pattern: pos(union(str, regex))) => arr(dictionary),
    split: (self: Self, separator: pos(union(str, regex))) => arr(str),
    replace: sig(
      (self: Self, pattern: pos(union(str, regex)), replacement: pos(str)) => str,
      // todo: match object
      (self: Self, pattern: pos(union(str, regex)), replacement: pos((match: pos(dictionary)) => str)) => str,
    ),
  ),
)


// === function ===
// :: state.at(selector)
// :: state.update(update)
// === any ===
// -> state.get
// -> state.at
// -> state.final
// :: state.update(update)
// === any ===
// -> state.get
// -> state.at
// -> state.final
// :: state.update(update)

// === element function ===

// :: selector.or(others)
// :: selector.and(others)
// :: selector.before(end)
// :: selector.after(start)
// :: figure(kind)
// :: query(target)

// === supplement function ===

// :: figure(supplement)
// :: heading(supplement)
// :: ref(supplement)
// :: equation(supplement)

// === numbering function ===

// :: figure(numbering)
// :: footnote(numbering)
// :: heading(numbering)
// :: enum(numbering)
// :: numbering(numbering)
// :: par.line(numbering)
// :: equation(numbering)
// :: page(numbering)
// :: counter.display(numbering)
// -> location.page-numbering

// === table cell customization ===

// :: table(fill)
// :: table(align)
// :: table(stroke)
// :: table(inset)
// :: cancel(angle)
// :: grid(fill)
// :: grid(align)
// :: grid(stroke)
// :: grid(inset)

// === table array

// :: table(columns)
// :: table(rows)
// :: table(gutter)
// :: table(column-gutter)
// :: table(row-gutter)
// :: table(fill)
// :: table(align)
// :: table(stroke)
// :: table(inset)

// === function ===

// -> content.func
// -> function.with
// :: plugin.transition(func)
// :: list(marker)
// :: outline(target)
// :: outline(indent)
// :: layout(func)
// -> gradient.kind
// :: counter.at(selector)
// :: counter.update(update)
// :: locate(selector)

// === array

// :: bibliography(sources)
// :: list(marker)
// :: document(author)
// :: document(keywords)
// :: enum(children)
// :: terms(children)
// :: raw(syntaxes)
// :: smartquote(quotes)
// :: text(font)
// :: text(stylistic-set)
// :: text(features)
// :: cases(delim)
// :: mat(delim)
// :: mat(rows)
// :: vec(delim)
// :: grid(columns)
// :: grid(rows)
// :: grid(gutter)
// :: grid(column-gutter)
// :: grid(row-gutter)
// :: grid(fill)
// :: grid(align)
// :: grid(stroke)
// :: grid(inset)
// -> color.components
// :: color.mix(colors)
// :: curve.move(start)
// :: curve.line(end)
// :: curve.quad(control)
// :: curve.quad(end)
// :: curve.cubic(control-start)
// :: curve.cubic(control-end)
// :: curve.cubic(end)
// :: gradient.linear(stops)
// :: gradient.radial(stops)
// :: gradient.radial(center)
// :: gradient.radial(focal-center)
// :: gradient.conic(stops)
// :: gradient.conic(center)
// -> gradient.stops
// -> gradient.center
// -> gradient.focal-center
// -> gradient.samples
// :: line(start)
// :: line(end)
// :: path(vertices)
// :: polygon(vertices)
// -> counter.get
// -> counter.at
// -> counter.final
// :: counter.update(update)
// -> query
// -> csv
// -> csv.decode

// === Dictionary

// :: link(dest)
// :: par(first-line-indent)
// :: smartquote(quotes)
// :: text(font)
// :: text(costs)
// :: text(features)
// :: mat(augment)
// -> measure
// :: page(margin)
// :: image(format)
// :: image.decode(format)
// -> location.position
// :: elem(attrs)

// === Radius

// :: highlight(radius)
// :: block(radius)
// :: box(radius)
// :: rect(radius)
// :: square(radius)

// === Outset

// :: block(outset)
// :: box(outset)
// :: circle(outset)
// :: ellipse(outset)
// :: rect(outset)
// :: square(outset)

// === Inset

// :: rect(inset)
// :: square(inset)
// :: ellipse(inset)
// :: table(inset)
// :: table.cell(inset)
// :: block(inset)
// :: box(inset)
// :: grid(inset)
// :: grid.cell(inset)
// :: circle(inset)

#let bytes-type = rec(
  name: "bytes",
  scope: (
    len: (self: Self) => int,
  ),
)

// === Stroke

// :: grid(stroke)
// :: grid.cell(stroke)
// :: grid.hline(stroke)
// :: grid.vline(stroke)
// :: circle(stroke)
// :: curve(stroke)
// :: ellipse(stroke)
// :: line(stroke)
// :: path(stroke)
// :: polygon(stroke)
// :: polygon.regular(stroke)
// :: rect(stroke)
// :: square(stroke)
// :: table(stroke)
// :: table.cell(stroke)
// :: table.hline(stroke)
// :: table.vline(stroke)
// :: highlight(stroke)
// :: overline(stroke)
// :: strike(stroke)
// :: text(stroke)
// :: underline(stroke)
// :: cancel(stroke)
// :: block(stroke)
// :: box(stroke)
// :: table(stroke)
// :: table.cell(stroke)
// :: table.hline(stroke)
// :: table.vline(stroke)
// :: highlight(stroke)
// :: overline(stroke)
// :: strike(stroke)
// :: text(stroke)
// :: underline(stroke)
// :: cancel(stroke)
// :: block(stroke)
// :: box(stroke)
// :: grid(stroke)
// :: grid.cell(stroke)
// :: grid.hline(stroke)
// :: grid.vline(stroke)
// :: circle(stroke)
// :: curve(stroke)
// :: ellipse(stroke)
// :: line(stroke)
// :: path(stroke)
// :: polygon(stroke)
// :: polygon.regular(stroke)
// :: rect(stroke)
// :: square(stroke)

// === content

// -> eval
// -> color.space
// :: color.negate(space)
// :: color.rotate(space)
// :: color.mix(space)
// :: gradient.linear(space)
// :: gradient.radial(space)
// :: gradient.conic(space)
// -> gradient.space
// -> counter.display
// -> xml
// -> xml.decode

#let assert = union(
  (condition: pos(bool), message: named(str)) => invariant(condition),
  eq: (left: pos(any), right: pos(any), message: named(str)) => invariant(eq(left, right)),
  ne: (left: pos(any), right: pos(any), message: named(str)) => invariant(neq(left, right)),
);

