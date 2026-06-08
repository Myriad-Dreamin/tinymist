#import "/typ/packages/typings/lib.typ": *

// Generated from Typst standard library type scopes.
// Parameter names, types, required/default flags, and pos/named/rest shape
// are aligned with `upstream::tests::std_types_snapshot`.
// Documentation is intentionally not embedded here.

#let _array-v = tv("V");
#let _array-u = tv("U");
#let _array-a = tv("A");

#let array = rec(
  name: "array",
  scope: (
    all: (self: Self.with(_array-v), test: pos((value: pos(_array-v)) => bool)) => bool,
    any: (self: Self.with(_array-v), test: pos((value: pos(_array-v)) => bool)) => bool,
    at: (self: Self.with(_array-v), index: pos(int), default: named(any, required: false)) => _array-v,
    chunks: (self: Self.with(_array-v), chunk-size: pos(int), exact: named(bool, required: false, default: false)) => Self.with(Self.with(_array-v)),
    contains: (self: Self.with(_array-v), value: pos(_array-v)) => bool,
    dedup: (self: Self.with(_array-v), key: named((value: pos(_array-v)) => any, required: false)) => Self.with(_array-v),
    enumerate: (self: Self.with(_array-v), start: named(int, required: false, default: 0)) => Self.with(tuple(int, _array-v)),
    filter: (self: Self.with(_array-v), test: pos((value: pos(_array-v)) => bool)) => Self.with(_array-v),
    find: (self: Self.with(_array-v), searcher: pos((value: pos(_array-v)) => bool)) => union(_array-v, none),
    first: (self: Self.with(_array-v), default: named(any, required: false)) => _array-v,
    flatten: (self: Self.with(_array-v)) => Self.with(any),
    fold: (self: Self.with(_array-v), init: pos(_array-a), folder: pos((acc: pos(_array-a), value: pos(_array-v)) => _array-a)) => _array-a,
    insert: (self: Self.with(_array-v), index: pos(int), value: pos(_array-v)) => none,
    intersperse: (self: Self.with(_array-v), separator: pos(_array-v)) => Self.with(_array-v),
    join: (self: Self.with(_array-v), separator: pos(union(any, none), required: false, default: none), last: named(any, required: false), default: named(union(any, none), required: false, default: none)) => any,
    last: (self: Self.with(_array-v), default: named(any, required: false)) => _array-v,
    len: (self: Self.with(_array-v)) => int,
    map: (self: Self.with(_array-v), mapper: pos((value: pos(_array-v)) => _array-u)) => Self.with(_array-u),
    pop: (self: Self.with(_array-v)) => _array-v,
    position: (self: Self.with(_array-v), searcher: pos((value: pos(_array-v)) => bool)) => union(int, none),
    product: (self: Self.with(_array-v), default: named(any, required: false)) => any,
    push: (self: Self.with(_array-v), value: pos(_array-v)) => none,
    range: (start: pos(int, required: false, default: 0), end: pos(int), step: named(int, required: false, default: 1)) => Self.with(int),
    reduce: (self: Self.with(_array-v), reducer: pos((acc: pos(_array-v), value: pos(_array-v)) => _array-v)) => _array-v,
    remove: (self: Self.with(_array-v), index: pos(int), default: named(any, required: false)) => _array-v,
    rev: (self: Self.with(_array-v)) => Self.with(_array-v),
    slice: (self: Self.with(_array-v), start: pos(int), end: pos(union(int, none), required: false, default: none), count: named(int, required: false)) => Self.with(_array-v),
    sorted: (self: Self.with(_array-v), key: named((value: pos(_array-v)) => any, required: false), by: named((left: pos(_array-v), right: pos(_array-v)) => bool, required: false)) => Self.with(_array-v),
    split: (self: Self.with(_array-v), at: pos(_array-v)) => Self.with(Self.with(_array-v)),
    sum: (self: Self.with(_array-v), default: named(any, required: false)) => any,
    to-dict: (self: Self.with(_array-v)) => dictionary,
    windows: (self: Self.with(_array-v), window-size: pos(int)) => Self.with(Self.with(_array-v)),
    zip: (self: Self.with(_array-v), exact: named(bool, required: false, default: false), others: rest(arr(array))) => array,
  ),
);

#let alignment = rec(
  name: "alignment",
  self: alignment,
  scope: (
    axis: (self: Self) => union("horizontal", "vertical", none),
    inv: (self: Self) => alignment,
  ),
);

#let angle = rec(
  name: "angle",
  self: angle,
  scope: (
    deg: (self: Self) => float,
    rad: (self: Self) => float,
  ),
);

#let bool = rec(
  name: "boolean",
  scope: (:),
);

#let bytes = rec(
  name: "bytes",
  self: bytes,
  scope: (
    at: (self: Self, index: pos(int), default: named(any, required: false)) => any,
    len: (self: Self) => int,
    slice: (self: Self, start: pos(int), end: pos(union(int, none), required: false, default: none), count: named(int, required: false)) => bytes,
  ),
);

#let color = rec(
  name: "color",
  self: color,
  scope: (
    cmyk: (cyan: pos(ratio), magenta: pos(ratio), yellow: pos(ratio), key: pos(ratio), color: pos(color)) => color,
    components: (self: Self, alpha: named(bool, required: false, default: true)) => array,
    darken: (self: Self, factor: pos(ratio)) => color,
    desaturate: (self: Self, factor: pos(ratio)) => color,
    hsl: (hue: pos(angle), saturation: pos(union(int, ratio)), lightness: pos(union(int, ratio)), alpha: pos(union(int, ratio)), color: pos(color)) => color,
    hsv: (hue: pos(angle), saturation: pos(union(int, ratio)), value: pos(union(int, ratio)), alpha: pos(union(int, ratio)), color: pos(color)) => color,
    lighten: (self: Self, factor: pos(ratio)) => color,
    linear-rgb: (red: pos(union(int, ratio)), green: pos(union(int, ratio)), blue: pos(union(int, ratio)), alpha: pos(union(int, ratio)), color: pos(color)) => color,
    luma: (lightness: pos(union(int, ratio)), alpha: pos(ratio), color: pos(color)) => color,
    mix: (colors: rest(arr(union(color, array))), space: named(any, required: false, default: code("oklab"))) => color,
    negate: (self: Self, space: named(any, required: false, default: code("oklab"))) => color,
    oklab: (lightness: pos(ratio), a: pos(union(float, ratio)), b: pos(union(float, ratio)), alpha: pos(ratio), color: pos(color)) => color,
    oklch: (lightness: pos(ratio), chroma: pos(union(float, ratio)), hue: pos(angle), alpha: pos(ratio), color: pos(color)) => color,
    opacify: (self: Self, scale: pos(ratio)) => color,
    rgb: (red: pos(union(int, ratio)), green: pos(union(int, ratio)), blue: pos(union(int, ratio)), alpha: pos(union(int, ratio)), hex: pos(str), color: pos(color)) => color,
    rotate: (self: Self, angle: pos(angle), space: named(any, required: false, default: code("oklch"))) => color,
    saturate: (self: Self, factor: pos(ratio)) => color,
    space: (self: Self) => any,
    to-hex: (self: Self) => str,
    transparentize: (self: Self, scale: pos(ratio)) => color,
  ),
);

#let content = rec(
  name: "content",
  self: content,
  scope: (
    at: (self: Self, field: pos(str), default: named(any, required: false)) => any,
    fields: (self: Self) => dictionary,
    func: (self: Self) => function,
    has: (self: Self, field: pos(str)) => bool,
    location: (self: Self) => union(location, none),
  ),
);

#let counter = rec(
  name: "counter",
  self: counter,
  scope: (
    at: (self: Self, selector: pos(union(label, function, location, selector))) => union(int, array),
    display: (self: Self, numbering: pos(union(str, function, auto), required: false, default: auto), both: named(bool, required: false, default: false)) => any,
    final: (self: Self) => union(int, array),
    get: (self: Self) => union(int, array),
    step: (self: Self, level: named(int, required: false, default: 1)) => content,
    update: (self: Self, update: pos(union(int, array, function))) => content,
  ),
);

#let datetime = rec(
  name: "datetime",
  self: datetime,
  scope: (
    day: (self: Self) => union(int, none),
    display: (self: Self, pattern: pos(union(str, auto), required: false, default: auto)) => str,
    hour: (self: Self) => union(int, none),
    minute: (self: Self) => union(int, none),
    month: (self: Self) => union(int, none),
    ordinal: (self: Self) => union(int, none),
    second: (self: Self) => union(int, none),
    today: (offset: named(union(int, auto), required: false, default: auto)) => datetime,
    weekday: (self: Self) => union(int, none),
    year: (self: Self) => union(int, none),
  ),
);

#let decimal = rec(
  name: "decimal",
  scope: (:),
);

#let dictionary = rec(
  name: "dictionary",
  self: dictionary,
  scope: (
    at: (self: Self, key: pos(str), default: named(any, required: false)) => any,
    insert: (self: Self, key: pos(str), value: pos(any)) => none,
    keys: (self: Self) => array,
    len: (self: Self) => int,
    pairs: (self: Self) => array,
    remove: (self: Self, key: pos(str), default: named(any, required: false)) => any,
    values: (self: Self) => array,
  ),
);

#let direction = rec(
  name: "direction",
  self: direction,
  scope: (
    axis: (self: Self) => union("horizontal", "vertical"),
    end: (self: Self) => alignment,
    from: (side: pos(alignment)) => direction,
    inv: (self: Self) => direction,
    sign: (self: Self) => int,
    start: (self: Self) => alignment,
    to: (side: pos(alignment)) => direction,
  ),
);

#let duration = rec(
  name: "duration",
  self: duration,
  scope: (
    days: (self: Self) => float,
    hours: (self: Self) => float,
    minutes: (self: Self) => float,
    seconds: (self: Self) => float,
    weeks: (self: Self) => float,
  ),
);

#let float = rec(
  name: "float",
  self: float,
  scope: (
    from-bytes: (bytes: pos(bytes), endian: named(union("big", "little"), required: false, default: "little")) => float,
    is-infinite: (self: Self) => bool,
    is-nan: (self: Self) => bool,
    signum: (self: Self) => float,
    to-bytes: (self: Self, endian: named(union("big", "little"), required: false, default: "little"), size: named(int, required: false, default: 8)) => bytes,
  ),
);

#let fraction = rec(
  name: "fraction",
  scope: (:),
);

#let function = rec(
  name: "function",
  self: function,
  scope: (
    where: (self: Self, fields: rest(arr(any))) => selector,
    with: (self: Self, arguments: rest(arr(any))) => function,
  ),
);

#let gradient = rec(
  name: "gradient",
  self: gradient,
  scope: (
    angle: (self: Self) => union(angle, none),
    center: (self: Self) => union(array, none),
    conic: (stops: rest(arr(union(color, array))), angle: named(angle, required: false, default: 0deg), space: named(any, required: false, default: code("oklab")), relative: named(union("self", "parent", auto), required: false, default: auto), center: named(array, required: false, default: (50%, 50%))) => gradient,
    focal-center: (self: Self) => union(array, none),
    focal-radius: (self: Self) => union(ratio, none),
    kind: (self: Self) => function,
    linear: (stops: rest(arr(union(color, array))), space: named(any, required: false, default: code("oklab")), relative: named(union("self", "parent", auto), required: false, default: auto), dir: pos(direction, required: false, default: code("ltr")), angle: pos(angle)) => gradient,
    radial: (stops: rest(arr(union(color, array))), space: named(any, required: false, default: code("oklab")), relative: named(union("self", "parent", auto), required: false, default: auto), center: named(array, required: false, default: (50%, 50%)), radius: named(ratio, required: false, default: 50%), focal-center: named(union(array, auto), required: false, default: auto), focal-radius: named(ratio, required: false, default: 0%)) => gradient,
    radius: (self: Self) => union(ratio, none),
    relative: (self: Self) => union("self", "parent", auto),
    repeat: (self: Self, repetitions: pos(int), mirror: named(bool, required: false, default: false)) => gradient,
    sample: (self: Self, t: pos(union(ratio, angle))) => color,
    samples: (self: Self, ts: rest(arr(union(ratio, angle)))) => array,
    sharp: (self: Self, steps: pos(int), smoothness: named(ratio, required: false, default: 0%)) => gradient,
    space: (self: Self) => any,
    stops: (self: Self) => array,
  ),
);

#let int = rec(
  name: "integer",
  self: int,
  scope: (
    bit-and: (self: Self, rhs: pos(int)) => int,
    bit-lshift: (self: Self, shift: pos(int)) => int,
    bit-not: (self: Self) => int,
    bit-or: (self: Self, rhs: pos(int)) => int,
    bit-rshift: (self: Self, shift: pos(int), logical: named(bool, required: false, default: false)) => int,
    bit-xor: (self: Self, rhs: pos(int)) => int,
    from-bytes: (bytes: pos(bytes), endian: named(union("big", "little"), required: false, default: "little"), signed: named(bool, required: false, default: true)) => int,
    signum: (self: Self) => int,
    to-bytes: (self: Self, endian: named(union("big", "little"), required: false, default: "little"), size: named(int, required: false, default: 8)) => bytes,
  ),
);

#let label = rec(
  name: "label",
  scope: (:),
);

#let length = rec(
  name: "length",
  self: length,
  scope: (
    cm: (self: Self) => float,
    inches: (self: Self) => float,
    mm: (self: Self) => float,
    pt: (self: Self) => float,
    to-absolute: (self: Self) => length,
  ),
);

#let location = rec(
  name: "location",
  self: location,
  scope: (
    page: (self: Self) => int,
    page-numbering: (self: Self) => union(str, function, none),
    position: (self: Self) => dictionary,
  ),
);

#let module = rec(
  name: "module",
  scope: (:),
);

#let ratio = rec(
  name: "ratio",
  scope: (:),
);

#let regex = rec(
  name: "regex",
  scope: (:),
);

#let relative = rec(
  name: "relative length",
  scope: (:),
);

#let selector = rec(
  name: "selector",
  self: selector,
  scope: (
    after: (self: Self, start: pos(union(label, function, location, selector)), inclusive: named(bool, required: false, default: true)) => selector,
    "and": (self: Self, others: rest(arr(union(str, function, label, regex, location, selector)))) => selector,
    before: (self: Self, end: pos(union(label, function, location, selector)), inclusive: named(bool, required: false, default: true)) => selector,
    "or": (self: Self, others: rest(arr(union(str, function, label, regex, location, selector)))) => selector,
  ),
);

#let state = rec(
  name: "state",
  self: state,
  scope: (
    at: (self: Self, selector: pos(union(label, function, location, selector))) => any,
    final: (self: Self) => any,
    get: (self: Self) => any,
    update: (self: Self, update: pos(union(function, any))) => content,
  ),
);

#let str = rec(
  name: "string",
  self: str,
  scope: (
    at: (self: Self, index: pos(int), default: named(any, required: false)) => any,
    clusters: (self: Self) => array,
    codepoints: (self: Self) => array,
    contains: (self: Self, pattern: pos(union(str, regex))) => bool,
    ends-with: (self: Self, pattern: pos(union(str, regex))) => bool,
    find: (self: Self, pattern: pos(union(str, regex))) => union(str, none),
    first: (self: Self, default: named(str, required: false)) => str,
    from-unicode: (value: pos(int)) => str,
    last: (self: Self, default: named(str, required: false)) => str,
    len: (self: Self) => int,
    match: (self: Self, pattern: pos(union(str, regex))) => union(dictionary, none),
    matches: (self: Self, pattern: pos(union(str, regex))) => array,
    normalize: (self: Self, form: named(union("nfc", "nfd", "nfkc", "nfkd"), required: false, default: "nfc")) => str,
    position: (self: Self, pattern: pos(union(str, regex))) => union(int, none),
    replace: (self: Self, pattern: pos(union(str, regex)), replacement: pos(union(str, function)), count: named(int, required: false)) => str,
    rev: (self: Self) => str,
    slice: (self: Self, start: pos(int), end: pos(union(int, none), required: false, default: none), count: named(int, required: false)) => str,
    split: (self: Self, pattern: pos(union(str, regex, none), required: false, default: none)) => array,
    starts-with: (self: Self, pattern: pos(union(str, regex))) => bool,
    to-unicode: (character: pos(str)) => int,
    trim: (self: Self, pattern: pos(union(str, regex, none), required: false, default: none), at: named(alignment, required: false), repeat: named(bool, required: false, default: true)) => str,
  ),
);

#let stroke = rec(
  name: "stroke",
  scope: (:),
);

#let symbol = rec(
  name: "symbol",
  scope: (:),
);

#let pattern = rec(
  name: "tiling",
  scope: (:),
);
#let tiling = pattern;

#let type = rec(
  name: "type",
  scope: (:),
);

#let version = rec(
  name: "version",
  self: version,
  scope: (
    at: (self: Self, index: pos(int)) => int,
  ),
);

// Backwards-compatible generic helpers used by existing fixtures.
#let array-type(V: _array-v) = array.with(V);
#let dict-type(V: any) = dictionary;
#let str-type = str;
