#import "/typ/packages/typings/lib.typ": *

// Generated from Typst standard library type scopes.
// Parameter names, types, required/default flags, and pos/named/rest shape
// are aligned with `upstream::tests::std_types_snapshot`.
// Documentation is intentionally not embedded here.

#let alignment = rec(
  name: "alignment",
  scope: (
    axis: (self: pos(alignment, required: true)) => union("horizontal", "vertical", none),
    inv: (self: pos(alignment, required: true)) => alignment,
  ),
);

#let angle = rec(
  name: "angle",
  scope: (
    deg: (self: pos(angle, required: true)) => float,
    rad: (self: pos(angle, required: true)) => float,
  ),
);

#let arguments = rec(
  name: "arguments",
  scope: (
    at: (self: pos(arguments, required: true), key: pos(union(int, str), required: true), default: named(any, required: false)) => any,
    named: (self: pos(arguments, required: true)) => dictionary,
    pos: (self: pos(arguments, required: true)) => array,
  ),
);

#let array = rec(
  name: "array",
  scope: (
    all: (self: pos(array, required: true), test: pos(function, required: true)) => bool,
    any: (self: pos(array, required: true), test: pos(function, required: true)) => bool,
    at: (self: pos(array, required: true), index: pos(int, required: true), default: named(any, required: false)) => any,
    chunks: (self: pos(array, required: true), chunk-size: pos(int, required: true), exact: named(bool, required: false, default: "false")) => array,
    contains: (self: pos(array, required: true), value: pos(any, required: true)) => bool,
    dedup: (self: pos(array, required: true), key: named(function, required: false)) => array,
    enumerate: (self: pos(array, required: true), start: named(int, required: false, default: "0")) => array,
    filter: (self: pos(array, required: true), test: pos(function, required: true)) => array,
    find: (self: pos(array, required: true), searcher: pos(function, required: true)) => union(any, none),
    first: (self: pos(array, required: true), default: named(any, required: false)) => any,
    flatten: (self: pos(array, required: true)) => array,
    fold: (self: pos(array, required: true), init: pos(any, required: true), folder: pos(function, required: true)) => any,
    insert: (self: pos(array, required: true), index: pos(int, required: true), value: pos(any, required: true)) => none,
    intersperse: (self: pos(array, required: true), separator: pos(any, required: true)) => array,
    join: (self: pos(array, required: true), separator: pos(union(any, none), required: false, default: "none"), last: named(any, required: false), default: named(union(any, none), required: false, default: "none")) => any,
    last: (self: pos(array, required: true), default: named(any, required: false)) => any,
    len: (self: pos(array, required: true)) => int,
    map: (self: pos(array, required: true), mapper: pos(function, required: true)) => array,
    pop: (self: pos(array, required: true)) => any,
    position: (self: pos(array, required: true), searcher: pos(function, required: true)) => union(int, none),
    product: (self: pos(array, required: true), default: named(any, required: false)) => any,
    push: (self: pos(array, required: true), value: pos(any, required: true)) => none,
    range: (start: pos(int, required: false, default: "0"), end: pos(int, required: true), step: named(int, required: false, default: "1")) => array,
    reduce: (self: pos(array, required: true), reducer: pos(function, required: true)) => any,
    remove: (self: pos(array, required: true), index: pos(int, required: true), default: named(any, required: false)) => any,
    rev: (self: pos(array, required: true)) => array,
    slice: (self: pos(array, required: true), start: pos(int, required: true), end: pos(union(int, none), required: false, default: "none"), count: named(int, required: false)) => array,
    sorted: (self: pos(array, required: true), key: named(function, required: false), by: named(function, required: false)) => array,
    split: (self: pos(array, required: true), at: pos(any, required: true)) => array,
    sum: (self: pos(array, required: true), default: named(any, required: false)) => any,
    to-dict: (self: pos(array, required: true)) => dictionary,
    windows: (self: pos(array, required: true), window-size: pos(int, required: true)) => array,
    zip: (self: pos(array, required: true), exact: named(bool, required: false, default: "false"), others: rest(array, required: true)) => array,
  ),
);

#let bool = rec(
  name: "boolean",
  scope: (:),
);

#let bytes = rec(
  name: "bytes",
  scope: (
    at: (self: pos(bytes, required: true), index: pos(int, required: true), default: named(any, required: false)) => any,
    len: (self: pos(bytes, required: true)) => int,
    slice: (self: pos(bytes, required: true), start: pos(int, required: true), end: pos(union(int, none), required: false, default: "none"), count: named(int, required: false)) => bytes,
  ),
);

#let color = rec(
  name: "color",
  scope: (
    cmyk: (cyan: pos(ratio, required: true), magenta: pos(ratio, required: true), yellow: pos(ratio, required: true), key: pos(ratio, required: true), color: pos(color, required: true)) => color,
    components: (self: pos(color, required: true), alpha: named(bool, required: false, default: "true")) => array,
    darken: (self: pos(color, required: true), factor: pos(ratio, required: true)) => color,
    desaturate: (self: pos(color, required: true), factor: pos(ratio, required: true)) => color,
    hsl: (hue: pos(angle, required: true), saturation: pos(union(int, ratio), required: true), lightness: pos(union(int, ratio), required: true), alpha: pos(union(int, ratio), required: true), color: pos(color, required: true)) => color,
    hsv: (hue: pos(angle, required: true), saturation: pos(union(int, ratio), required: true), value: pos(union(int, ratio), required: true), alpha: pos(union(int, ratio), required: true), color: pos(color, required: true)) => color,
    lighten: (self: pos(color, required: true), factor: pos(ratio, required: true)) => color,
    linear-rgb: (red: pos(union(int, ratio), required: true), green: pos(union(int, ratio), required: true), blue: pos(union(int, ratio), required: true), alpha: pos(union(int, ratio), required: true), color: pos(color, required: true)) => color,
    luma: (lightness: pos(union(int, ratio), required: true), alpha: pos(ratio, required: true), color: pos(color, required: true)) => color,
    mix: (colors: rest(union(color, array), required: true), space: named(any, required: false, default: "oklab")) => color,
    negate: (self: pos(color, required: true), space: named(any, required: false, default: "oklab")) => color,
    oklab: (lightness: pos(ratio, required: true), a: pos(union(float, ratio), required: true), b: pos(union(float, ratio), required: true), alpha: pos(ratio, required: true), color: pos(color, required: true)) => color,
    oklch: (lightness: pos(ratio, required: true), chroma: pos(union(float, ratio), required: true), hue: pos(angle, required: true), alpha: pos(ratio, required: true), color: pos(color, required: true)) => color,
    opacify: (self: pos(color, required: true), scale: pos(ratio, required: true)) => color,
    rgb: (red: pos(union(int, ratio), required: true), green: pos(union(int, ratio), required: true), blue: pos(union(int, ratio), required: true), alpha: pos(union(int, ratio), required: true), hex: pos(str, required: true), color: pos(color, required: true)) => color,
    rotate: (self: pos(color, required: true), angle: pos(angle, required: true), space: named(any, required: false, default: "oklch")) => color,
    saturate: (self: pos(color, required: true), factor: pos(ratio, required: true)) => color,
    space: (self: pos(color, required: true)) => any,
    to-hex: (self: pos(color, required: true)) => str,
    transparentize: (self: pos(color, required: true), scale: pos(ratio, required: true)) => color,
  ),
);

#let content = rec(
  name: "content",
  scope: (
    at: (self: pos(content, required: true), field: pos(str, required: true), default: named(any, required: false)) => any,
    fields: (self: pos(content, required: true)) => dictionary,
    func: (self: pos(content, required: true)) => function,
    has: (self: pos(content, required: true), field: pos(str, required: true)) => bool,
    location: (self: pos(content, required: true)) => union(location, none),
  ),
);

#let counter = rec(
  name: "counter",
  scope: (
    at: (self: pos(counter, required: true), selector: pos(union(label, function, location, selector), required: true)) => union(int, array),
    display: (self: pos(counter, required: true), numbering: pos(union(str, function, auto), required: false, default: "auto"), both: named(bool, required: false, default: "false")) => any,
    final: (self: pos(counter, required: true)) => union(int, array),
    get: (self: pos(counter, required: true)) => union(int, array),
    step: (self: pos(counter, required: true), level: named(int, required: false, default: "1")) => content,
    update: (self: pos(counter, required: true), update: pos(union(int, array, function), required: true)) => content,
  ),
);

#let datetime = rec(
  name: "datetime",
  scope: (
    day: (self: pos(datetime, required: true)) => union(int, none),
    display: (self: pos(datetime, required: true), pattern: pos(union(str, auto), required: false, default: "auto")) => str,
    hour: (self: pos(datetime, required: true)) => union(int, none),
    minute: (self: pos(datetime, required: true)) => union(int, none),
    month: (self: pos(datetime, required: true)) => union(int, none),
    ordinal: (self: pos(datetime, required: true)) => union(int, none),
    second: (self: pos(datetime, required: true)) => union(int, none),
    today: (offset: named(union(int, auto), required: false, default: "auto")) => datetime,
    weekday: (self: pos(datetime, required: true)) => union(int, none),
    year: (self: pos(datetime, required: true)) => union(int, none),
  ),
);

#let decimal = rec(
  name: "decimal",
  scope: (:),
);

#let dictionary = rec(
  name: "dictionary",
  scope: (
    at: (self: pos(dictionary, required: true), key: pos(str, required: true), default: named(any, required: false)) => any,
    insert: (self: pos(dictionary, required: true), key: pos(str, required: true), value: pos(any, required: true)) => none,
    keys: (self: pos(dictionary, required: true)) => array,
    len: (self: pos(dictionary, required: true)) => int,
    pairs: (self: pos(dictionary, required: true)) => array,
    remove: (self: pos(dictionary, required: true), key: pos(str, required: true), default: named(any, required: false)) => any,
    values: (self: pos(dictionary, required: true)) => array,
  ),
);

#let direction = rec(
  name: "direction",
  scope: (
    axis: (self: pos(direction, required: true)) => union("horizontal", "vertical"),
    end: (self: pos(direction, required: true)) => alignment,
    from: (side: pos(alignment, required: true)) => direction,
    inv: (self: pos(direction, required: true)) => direction,
    sign: (self: pos(direction, required: true)) => int,
    start: (self: pos(direction, required: true)) => alignment,
    to: (side: pos(alignment, required: true)) => direction,
  ),
);

#let duration = rec(
  name: "duration",
  scope: (
    days: (self: pos(duration, required: true)) => float,
    hours: (self: pos(duration, required: true)) => float,
    minutes: (self: pos(duration, required: true)) => float,
    seconds: (self: pos(duration, required: true)) => float,
    weeks: (self: pos(duration, required: true)) => float,
  ),
);

#let float = rec(
  name: "float",
  scope: (
    from-bytes: (bytes: pos(bytes, required: true), endian: named(union("big", "little"), required: false, default: "\"little\"")) => float,
    is-infinite: (self: pos(float, required: true)) => bool,
    is-nan: (self: pos(float, required: true)) => bool,
    signum: (self: pos(float, required: true)) => float,
    to-bytes: (self: pos(float, required: true), endian: named(union("big", "little"), required: false, default: "\"little\""), size: named(int, required: false, default: "8")) => bytes,
  ),
);

#let fraction = rec(
  name: "fraction",
  scope: (:),
);

#let function = rec(
  name: "function",
  scope: (
    where: (self: pos(function, required: true), fields: rest(any, required: true)) => selector,
    with: (self: pos(function, required: true), arguments: rest(any, required: true)) => function,
  ),
);

#let gradient = rec(
  name: "gradient",
  scope: (
    angle: (self: pos(gradient, required: true)) => union(angle, none),
    center: (self: pos(gradient, required: true)) => union(array, none),
    conic: (stops: rest(union(color, array), required: true), angle: named(angle, required: false, default: "0deg"), space: named(any, required: false, default: "oklab"), relative: named(union("self", "parent", auto), required: false, default: "auto"), center: named(array, required: false, default: "(50%, 50%)")) => gradient,
    focal-center: (self: pos(gradient, required: true)) => union(array, none),
    focal-radius: (self: pos(gradient, required: true)) => union(ratio, none),
    kind: (self: pos(gradient, required: true)) => function,
    linear: (stops: rest(union(color, array), required: true), space: named(any, required: false, default: "oklab"), relative: named(union("self", "parent", auto), required: false, default: "auto"), dir: pos(direction, required: false, default: "ltr"), angle: pos(angle, required: true)) => gradient,
    radial: (stops: rest(union(color, array), required: true), space: named(any, required: false, default: "oklab"), relative: named(union("self", "parent", auto), required: false, default: "auto"), center: named(array, required: false, default: "(50%, 50%)"), radius: named(ratio, required: false, default: "50%"), focal-center: named(union(array, auto), required: false, default: "auto"), focal-radius: named(ratio, required: false, default: "0%")) => gradient,
    radius: (self: pos(gradient, required: true)) => union(ratio, none),
    relative: (self: pos(gradient, required: true)) => union("self", "parent", auto),
    repeat: (self: pos(gradient, required: true), repetitions: pos(int, required: true), mirror: named(bool, required: false, default: "false")) => gradient,
    sample: (self: pos(gradient, required: true), t: pos(union(ratio, angle), required: true)) => color,
    samples: (self: pos(gradient, required: true), ts: rest(union(ratio, angle), required: true)) => array,
    sharp: (self: pos(gradient, required: true), steps: pos(int, required: true), smoothness: named(ratio, required: false, default: "0%")) => gradient,
    space: (self: pos(gradient, required: true)) => any,
    stops: (self: pos(gradient, required: true)) => array,
  ),
);

#let int = rec(
  name: "integer",
  scope: (
    bit-and: (self: pos(int, required: true), rhs: pos(int, required: true)) => int,
    bit-lshift: (self: pos(int, required: true), shift: pos(int, required: true)) => int,
    bit-not: (self: pos(int, required: true)) => int,
    bit-or: (self: pos(int, required: true), rhs: pos(int, required: true)) => int,
    bit-rshift: (self: pos(int, required: true), shift: pos(int, required: true), logical: named(bool, required: false, default: "false")) => int,
    bit-xor: (self: pos(int, required: true), rhs: pos(int, required: true)) => int,
    from-bytes: (bytes: pos(bytes, required: true), endian: named(union("big", "little"), required: false, default: "\"little\""), signed: named(bool, required: false, default: "true")) => int,
    signum: (self: pos(int, required: true)) => int,
    to-bytes: (self: pos(int, required: true), endian: named(union("big", "little"), required: false, default: "\"little\""), size: named(int, required: false, default: "8")) => bytes,
  ),
);

#let label = rec(
  name: "label",
  scope: (:),
);

#let length = rec(
  name: "length",
  scope: (
    cm: (self: pos(length, required: true)) => float,
    inches: (self: pos(length, required: true)) => float,
    mm: (self: pos(length, required: true)) => float,
    pt: (self: pos(length, required: true)) => float,
    to-absolute: (self: pos(length, required: true)) => length,
  ),
);

#let location = rec(
  name: "location",
  scope: (
    page: (self: pos(location, required: true)) => int,
    page-numbering: (self: pos(location, required: true)) => union(str, function, none),
    position: (self: pos(location, required: true)) => dictionary,
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
  scope: (
    after: (self: pos(union(str, function, label, regex, location, selector), required: true), start: pos(union(label, function, location, selector), required: true), inclusive: named(bool, required: false, default: "true")) => selector,
    "and": (self: pos(union(str, function, label, regex, location, selector), required: true), others: rest(union(str, function, label, regex, location, selector), required: true)) => selector,
    before: (self: pos(union(str, function, label, regex, location, selector), required: true), end: pos(union(label, function, location, selector), required: true), inclusive: named(bool, required: false, default: "true")) => selector,
    "or": (self: pos(union(str, function, label, regex, location, selector), required: true), others: rest(union(str, function, label, regex, location, selector), required: true)) => selector,
  ),
);

#let state = rec(
  name: "state",
  scope: (
    at: (self: pos(state, required: true), selector: pos(union(label, function, location, selector), required: true)) => any,
    final: (self: pos(state, required: true)) => any,
    get: (self: pos(state, required: true)) => any,
    update: (self: pos(state, required: true), update: pos(union(function, any), required: true)) => content,
  ),
);

#let str = rec(
  name: "string",
  scope: (
    at: (self: pos(str, required: true), index: pos(int, required: true), default: named(any, required: false)) => any,
    clusters: (self: pos(str, required: true)) => array,
    codepoints: (self: pos(str, required: true)) => array,
    contains: (self: pos(str, required: true), pattern: pos(union(str, regex), required: true)) => bool,
    ends-with: (self: pos(str, required: true), pattern: pos(union(str, regex), required: true)) => bool,
    find: (self: pos(str, required: true), pattern: pos(union(str, regex), required: true)) => union(str, none),
    first: (self: pos(str, required: true), default: named(str, required: false)) => str,
    from-unicode: (value: pos(int, required: true)) => str,
    last: (self: pos(str, required: true), default: named(str, required: false)) => str,
    len: (self: pos(str, required: true)) => int,
    match: (self: pos(str, required: true), pattern: pos(union(str, regex), required: true)) => union(dictionary, none),
    matches: (self: pos(str, required: true), pattern: pos(union(str, regex), required: true)) => array,
    normalize: (self: pos(str, required: true), form: named(union("nfc", "nfd", "nfkc", "nfkd"), required: false, default: "\"nfc\"")) => str,
    position: (self: pos(str, required: true), pattern: pos(union(str, regex), required: true)) => union(int, none),
    replace: (self: pos(str, required: true), pattern: pos(union(str, regex), required: true), replacement: pos(union(str, function), required: true), count: named(int, required: false)) => str,
    rev: (self: pos(str, required: true)) => str,
    slice: (self: pos(str, required: true), start: pos(int, required: true), end: pos(union(int, none), required: false, default: "none"), count: named(int, required: false)) => str,
    split: (self: pos(str, required: true), pattern: pos(union(str, regex, none), required: false, default: "none")) => array,
    starts-with: (self: pos(str, required: true), pattern: pos(union(str, regex), required: true)) => bool,
    to-unicode: (character: pos(str, required: true)) => int,
    trim: (self: pos(str, required: true), pattern: pos(union(str, regex, none), required: false, default: "none"), at: named(alignment, required: false), repeat: named(bool, required: false, default: "true")) => str,
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
  scope: (
    at: (self: pos(version, required: true), index: pos(int, required: true)) => int,
  ),
);

// Backwards-compatible generic helpers used by existing fixtures.
#let array-type(V: any) = array;
#let dict-type(V: any) = dictionary;
#let str-type = str;
