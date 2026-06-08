#import "/typ/packages/typings/lib.typ": *

// Generated from Typst standard library type scopes.
// Parameter names, types, required/default flags, and pos/named/rest shape
// are aligned with `upstream::tests::std_types_snapshot`.
// Documentation is intentionally not embedded here.

#let alignment = rec(
  name: "alignment",
  scope: (
    axis: (self: pos(alignment)) => union("horizontal", "vertical", none),
    inv: (self: pos(alignment)) => alignment,
  ),
);

#let angle = rec(
  name: "angle",
  scope: (
    deg: (self: pos(angle)) => float,
    rad: (self: pos(angle)) => float,
  ),
);

#let arguments = rec(
  name: "arguments",
  scope: (
    at: (self: pos(arguments), key: pos(union(int, str)), default: named(any, required: false)) => any,
    named: (self: pos(arguments)) => dictionary,
    pos: (self: pos(arguments)) => array,
  ),
);

#let array = rec(
  name: "array",
  scope: (
    all: (self: pos(array), test: pos(function)) => bool,
    any: (self: pos(array), test: pos(function)) => bool,
    at: (self: pos(array), index: pos(int), default: named(any, required: false)) => any,
    chunks: (self: pos(array), chunk-size: pos(int), exact: named(bool, required: false, default: "false")) => array,
    contains: (self: pos(array), value: pos(any)) => bool,
    dedup: (self: pos(array), key: named(function, required: false)) => array,
    enumerate: (self: pos(array), start: named(int, required: false, default: "0")) => array,
    filter: (self: pos(array), test: pos(function)) => array,
    find: (self: pos(array), searcher: pos(function)) => union(any, none),
    first: (self: pos(array), default: named(any, required: false)) => any,
    flatten: (self: pos(array)) => array,
    fold: (self: pos(array), init: pos(any), folder: pos(function)) => any,
    insert: (self: pos(array), index: pos(int), value: pos(any)) => none,
    intersperse: (self: pos(array), separator: pos(any)) => array,
    join: (self: pos(array), separator: pos(union(any, none), required: false, default: "none"), last: named(any, required: false), default: named(union(any, none), required: false, default: "none")) => any,
    last: (self: pos(array), default: named(any, required: false)) => any,
    len: (self: pos(array)) => int,
    map: (self: pos(array), mapper: pos(function)) => array,
    pop: (self: pos(array)) => any,
    position: (self: pos(array), searcher: pos(function)) => union(int, none),
    product: (self: pos(array), default: named(any, required: false)) => any,
    push: (self: pos(array), value: pos(any)) => none,
    range: (start: pos(int, required: false, default: "0"), end: pos(int), step: named(int, required: false, default: "1")) => array,
    reduce: (self: pos(array), reducer: pos(function)) => any,
    remove: (self: pos(array), index: pos(int), default: named(any, required: false)) => any,
    rev: (self: pos(array)) => array,
    slice: (self: pos(array), start: pos(int), end: pos(union(int, none), required: false, default: "none"), count: named(int, required: false)) => array,
    sorted: (self: pos(array), key: named(function, required: false), by: named(function, required: false)) => array,
    split: (self: pos(array), at: pos(any)) => array,
    sum: (self: pos(array), default: named(any, required: false)) => any,
    to-dict: (self: pos(array)) => dictionary,
    windows: (self: pos(array), window-size: pos(int)) => array,
    zip: (self: pos(array), exact: named(bool, required: false, default: "false"), others: rest(arr(array))) => array,
  ),
);

#let bool = rec(
  name: "boolean",
  scope: (:),
);

#let bytes = rec(
  name: "bytes",
  scope: (
    at: (self: pos(bytes), index: pos(int), default: named(any, required: false)) => any,
    len: (self: pos(bytes)) => int,
    slice: (self: pos(bytes), start: pos(int), end: pos(union(int, none), required: false, default: "none"), count: named(int, required: false)) => bytes,
  ),
);

#let color = rec(
  name: "color",
  scope: (
    cmyk: (cyan: pos(ratio), magenta: pos(ratio), yellow: pos(ratio), key: pos(ratio), color: pos(color)) => color,
    components: (self: pos(color), alpha: named(bool, required: false, default: "true")) => array,
    darken: (self: pos(color), factor: pos(ratio)) => color,
    desaturate: (self: pos(color), factor: pos(ratio)) => color,
    hsl: (hue: pos(angle), saturation: pos(union(int, ratio)), lightness: pos(union(int, ratio)), alpha: pos(union(int, ratio)), color: pos(color)) => color,
    hsv: (hue: pos(angle), saturation: pos(union(int, ratio)), value: pos(union(int, ratio)), alpha: pos(union(int, ratio)), color: pos(color)) => color,
    lighten: (self: pos(color), factor: pos(ratio)) => color,
    linear-rgb: (red: pos(union(int, ratio)), green: pos(union(int, ratio)), blue: pos(union(int, ratio)), alpha: pos(union(int, ratio)), color: pos(color)) => color,
    luma: (lightness: pos(union(int, ratio)), alpha: pos(ratio), color: pos(color)) => color,
    mix: (colors: rest(arr(union(color, array))), space: named(any, required: false, default: "oklab")) => color,
    negate: (self: pos(color), space: named(any, required: false, default: "oklab")) => color,
    oklab: (lightness: pos(ratio), a: pos(union(float, ratio)), b: pos(union(float, ratio)), alpha: pos(ratio), color: pos(color)) => color,
    oklch: (lightness: pos(ratio), chroma: pos(union(float, ratio)), hue: pos(angle), alpha: pos(ratio), color: pos(color)) => color,
    opacify: (self: pos(color), scale: pos(ratio)) => color,
    rgb: (red: pos(union(int, ratio)), green: pos(union(int, ratio)), blue: pos(union(int, ratio)), alpha: pos(union(int, ratio)), hex: pos(str), color: pos(color)) => color,
    rotate: (self: pos(color), angle: pos(angle), space: named(any, required: false, default: "oklch")) => color,
    saturate: (self: pos(color), factor: pos(ratio)) => color,
    space: (self: pos(color)) => any,
    to-hex: (self: pos(color)) => str,
    transparentize: (self: pos(color), scale: pos(ratio)) => color,
  ),
);

#let content = rec(
  name: "content",
  scope: (
    at: (self: pos(content), field: pos(str), default: named(any, required: false)) => any,
    fields: (self: pos(content)) => dictionary,
    func: (self: pos(content)) => function,
    has: (self: pos(content), field: pos(str)) => bool,
    location: (self: pos(content)) => union(location, none),
  ),
);

#let counter = rec(
  name: "counter",
  scope: (
    at: (self: pos(counter), selector: pos(union(label, function, location, selector))) => union(int, array),
    display: (self: pos(counter), numbering: pos(union(str, function, auto), required: false, default: "auto"), both: named(bool, required: false, default: "false")) => any,
    final: (self: pos(counter)) => union(int, array),
    get: (self: pos(counter)) => union(int, array),
    step: (self: pos(counter), level: named(int, required: false, default: "1")) => content,
    update: (self: pos(counter), update: pos(union(int, array, function))) => content,
  ),
);

#let datetime = rec(
  name: "datetime",
  scope: (
    day: (self: pos(datetime)) => union(int, none),
    display: (self: pos(datetime), pattern: pos(union(str, auto), required: false, default: "auto")) => str,
    hour: (self: pos(datetime)) => union(int, none),
    minute: (self: pos(datetime)) => union(int, none),
    month: (self: pos(datetime)) => union(int, none),
    ordinal: (self: pos(datetime)) => union(int, none),
    second: (self: pos(datetime)) => union(int, none),
    today: (offset: named(union(int, auto), required: false, default: "auto")) => datetime,
    weekday: (self: pos(datetime)) => union(int, none),
    year: (self: pos(datetime)) => union(int, none),
  ),
);

#let decimal = rec(
  name: "decimal",
  scope: (:),
);

#let dictionary = rec(
  name: "dictionary",
  scope: (
    at: (self: pos(dictionary), key: pos(str), default: named(any, required: false)) => any,
    insert: (self: pos(dictionary), key: pos(str), value: pos(any)) => none,
    keys: (self: pos(dictionary)) => array,
    len: (self: pos(dictionary)) => int,
    pairs: (self: pos(dictionary)) => array,
    remove: (self: pos(dictionary), key: pos(str), default: named(any, required: false)) => any,
    values: (self: pos(dictionary)) => array,
  ),
);

#let direction = rec(
  name: "direction",
  scope: (
    axis: (self: pos(direction)) => union("horizontal", "vertical"),
    end: (self: pos(direction)) => alignment,
    from: (side: pos(alignment)) => direction,
    inv: (self: pos(direction)) => direction,
    sign: (self: pos(direction)) => int,
    start: (self: pos(direction)) => alignment,
    to: (side: pos(alignment)) => direction,
  ),
);

#let duration = rec(
  name: "duration",
  scope: (
    days: (self: pos(duration)) => float,
    hours: (self: pos(duration)) => float,
    minutes: (self: pos(duration)) => float,
    seconds: (self: pos(duration)) => float,
    weeks: (self: pos(duration)) => float,
  ),
);

#let float = rec(
  name: "float",
  scope: (
    from-bytes: (bytes: pos(bytes), endian: named(union("big", "little"), required: false, default: "\"little\"")) => float,
    is-infinite: (self: pos(float)) => bool,
    is-nan: (self: pos(float)) => bool,
    signum: (self: pos(float)) => float,
    to-bytes: (self: pos(float), endian: named(union("big", "little"), required: false, default: "\"little\""), size: named(int, required: false, default: "8")) => bytes,
  ),
);

#let fraction = rec(
  name: "fraction",
  scope: (:),
);

#let function = rec(
  name: "function",
  scope: (
    where: (self: pos(function), fields: rest(arr(any))) => selector,
    with: (self: pos(function), arguments: rest(arr(any))) => function,
  ),
);

#let gradient = rec(
  name: "gradient",
  scope: (
    angle: (self: pos(gradient)) => union(angle, none),
    center: (self: pos(gradient)) => union(array, none),
    conic: (stops: rest(arr(union(color, array))), angle: named(angle, required: false, default: "0deg"), space: named(any, required: false, default: "oklab"), relative: named(union("self", "parent", auto), required: false, default: "auto"), center: named(array, required: false, default: "(50%, 50%)")) => gradient,
    focal-center: (self: pos(gradient)) => union(array, none),
    focal-radius: (self: pos(gradient)) => union(ratio, none),
    kind: (self: pos(gradient)) => function,
    linear: (stops: rest(arr(union(color, array))), space: named(any, required: false, default: "oklab"), relative: named(union("self", "parent", auto), required: false, default: "auto"), dir: pos(direction, required: false, default: "ltr"), angle: pos(angle)) => gradient,
    radial: (stops: rest(arr(union(color, array))), space: named(any, required: false, default: "oklab"), relative: named(union("self", "parent", auto), required: false, default: "auto"), center: named(array, required: false, default: "(50%, 50%)"), radius: named(ratio, required: false, default: "50%"), focal-center: named(union(array, auto), required: false, default: "auto"), focal-radius: named(ratio, required: false, default: "0%")) => gradient,
    radius: (self: pos(gradient)) => union(ratio, none),
    relative: (self: pos(gradient)) => union("self", "parent", auto),
    repeat: (self: pos(gradient), repetitions: pos(int), mirror: named(bool, required: false, default: "false")) => gradient,
    sample: (self: pos(gradient), t: pos(union(ratio, angle))) => color,
    samples: (self: pos(gradient), ts: rest(arr(union(ratio, angle)))) => array,
    sharp: (self: pos(gradient), steps: pos(int), smoothness: named(ratio, required: false, default: "0%")) => gradient,
    space: (self: pos(gradient)) => any,
    stops: (self: pos(gradient)) => array,
  ),
);

#let int = rec(
  name: "integer",
  scope: (
    bit-and: (self: pos(int), rhs: pos(int)) => int,
    bit-lshift: (self: pos(int), shift: pos(int)) => int,
    bit-not: (self: pos(int)) => int,
    bit-or: (self: pos(int), rhs: pos(int)) => int,
    bit-rshift: (self: pos(int), shift: pos(int), logical: named(bool, required: false, default: "false")) => int,
    bit-xor: (self: pos(int), rhs: pos(int)) => int,
    from-bytes: (bytes: pos(bytes), endian: named(union("big", "little"), required: false, default: "\"little\""), signed: named(bool, required: false, default: "true")) => int,
    signum: (self: pos(int)) => int,
    to-bytes: (self: pos(int), endian: named(union("big", "little"), required: false, default: "\"little\""), size: named(int, required: false, default: "8")) => bytes,
  ),
);

#let label = rec(
  name: "label",
  scope: (:),
);

#let length = rec(
  name: "length",
  scope: (
    cm: (self: pos(length)) => float,
    inches: (self: pos(length)) => float,
    mm: (self: pos(length)) => float,
    pt: (self: pos(length)) => float,
    to-absolute: (self: pos(length)) => length,
  ),
);

#let location = rec(
  name: "location",
  scope: (
    page: (self: pos(location)) => int,
    page-numbering: (self: pos(location)) => union(str, function, none),
    position: (self: pos(location)) => dictionary,
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
    after: (self: pos(union(str, function, label, regex, location, selector)), start: pos(union(label, function, location, selector)), inclusive: named(bool, required: false, default: "true")) => selector,
    "and": (self: pos(union(str, function, label, regex, location, selector)), others: rest(arr(union(str, function, label, regex, location, selector)))) => selector,
    before: (self: pos(union(str, function, label, regex, location, selector)), end: pos(union(label, function, location, selector)), inclusive: named(bool, required: false, default: "true")) => selector,
    "or": (self: pos(union(str, function, label, regex, location, selector)), others: rest(arr(union(str, function, label, regex, location, selector)))) => selector,
  ),
);

#let state = rec(
  name: "state",
  scope: (
    at: (self: pos(state), selector: pos(union(label, function, location, selector))) => any,
    final: (self: pos(state)) => any,
    get: (self: pos(state)) => any,
    update: (self: pos(state), update: pos(union(function, any))) => content,
  ),
);

#let str = rec(
  name: "string",
  scope: (
    at: (self: pos(str), index: pos(int), default: named(any, required: false)) => any,
    clusters: (self: pos(str)) => array,
    codepoints: (self: pos(str)) => array,
    contains: (self: pos(str), pattern: pos(union(str, regex))) => bool,
    ends-with: (self: pos(str), pattern: pos(union(str, regex))) => bool,
    find: (self: pos(str), pattern: pos(union(str, regex))) => union(str, none),
    first: (self: pos(str), default: named(str, required: false)) => str,
    from-unicode: (value: pos(int)) => str,
    last: (self: pos(str), default: named(str, required: false)) => str,
    len: (self: pos(str)) => int,
    match: (self: pos(str), pattern: pos(union(str, regex))) => union(dictionary, none),
    matches: (self: pos(str), pattern: pos(union(str, regex))) => array,
    normalize: (self: pos(str), form: named(union("nfc", "nfd", "nfkc", "nfkd"), required: false, default: "\"nfc\"")) => str,
    position: (self: pos(str), pattern: pos(union(str, regex))) => union(int, none),
    replace: (self: pos(str), pattern: pos(union(str, regex)), replacement: pos(union(str, function)), count: named(int, required: false)) => str,
    rev: (self: pos(str)) => str,
    slice: (self: pos(str), start: pos(int), end: pos(union(int, none), required: false, default: "none"), count: named(int, required: false)) => str,
    split: (self: pos(str), pattern: pos(union(str, regex, none), required: false, default: "none")) => array,
    starts-with: (self: pos(str), pattern: pos(union(str, regex))) => bool,
    to-unicode: (character: pos(str)) => int,
    trim: (self: pos(str), pattern: pos(union(str, regex, none), required: false, default: "none"), at: named(alignment, required: false), repeat: named(bool, required: false, default: "true")) => str,
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
    at: (self: pos(version), index: pos(int)) => int,
  ),
);

// Backwards-compatible generic helpers used by existing fixtures.
#let array-type(V: any) = array;
#let dict-type(V: any) = dictionary;
#let str-type = str;
