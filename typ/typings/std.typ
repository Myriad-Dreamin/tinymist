#import "/typ/packages/typings/lib.typ": *

// Generated from Typst standard library type scopes.
// Keep method parameter shapes aligned with `upstream::tests::std_types_snapshot`.
// The types are intentionally loose; this file preserves method names and
// parameter pos/named/rest shape for the typing parser.

#let alignment = rec(
  name: "alignment",
  scope: (
    axis: (self: pos(any)) => any,
    inv: (self: pos(any)) => any,
  ),
);

#let angle = rec(
  name: "angle",
  scope: (
    deg: (self: pos(any)) => any,
    rad: (self: pos(any)) => any,
  ),
);

#let arguments = rec(
  name: "arguments",
  scope: (
    at: (self: pos(any), key: pos(any), default: named(any)) => any,
    named: (self: pos(any)) => any,
    pos: (self: pos(any)) => any,
  ),
);

#let array = rec(
  name: "array",
  scope: (
    all: (self: pos(any), test: pos(any)) => any,
    any: (self: pos(any), test: pos(any)) => any,
    at: (self: pos(any), index: pos(any), default: named(any)) => any,
    chunks: (self: pos(any), chunk-size: pos(any), exact: named(any)) => any,
    contains: (self: pos(any), value: pos(any)) => any,
    dedup: (self: pos(any), key: named(any)) => any,
    enumerate: (self: pos(any), start: named(any)) => any,
    filter: (self: pos(any), test: pos(any)) => any,
    find: (self: pos(any), searcher: pos(any)) => any,
    first: (self: pos(any), default: named(any)) => any,
    flatten: (self: pos(any)) => any,
    fold: (self: pos(any), init: pos(any), folder: pos(any)) => any,
    insert: (self: pos(any), index: pos(any), value: pos(any)) => any,
    intersperse: (self: pos(any), separator: pos(any)) => any,
    join: (self: pos(any), separator: pos(any), last: named(any), default: named(any)) => any,
    last: (self: pos(any), default: named(any)) => any,
    len: (self: pos(any)) => any,
    map: (self: pos(any), mapper: pos(any)) => any,
    pop: (self: pos(any)) => any,
    position: (self: pos(any), searcher: pos(any)) => any,
    product: (self: pos(any), default: named(any)) => any,
    push: (self: pos(any), value: pos(any)) => any,
    range: (start: pos(any), end: pos(any), step: named(any)) => any,
    reduce: (self: pos(any), reducer: pos(any)) => any,
    remove: (self: pos(any), index: pos(any), default: named(any)) => any,
    rev: (self: pos(any)) => any,
    slice: (self: pos(any), start: pos(any), end: pos(any), count: named(any)) => any,
    sorted: (self: pos(any), key: named(any), by: named(any)) => any,
    split: (self: pos(any), at: pos(any)) => any,
    sum: (self: pos(any), default: named(any)) => any,
    to-dict: (self: pos(any)) => any,
    windows: (self: pos(any), window-size: pos(any)) => any,
    zip: (self: pos(any), exact: named(any), others: rest(any)) => any,
  ),
);

#let bool = rec(
  name: "boolean",
  scope: (:),
);

#let bytes = rec(
  name: "bytes",
  scope: (
    at: (self: pos(any), index: pos(any), default: named(any)) => any,
    len: (self: pos(any)) => any,
    slice: (self: pos(any), start: pos(any), end: pos(any), count: named(any)) => any,
  ),
);

#let color = rec(
  name: "color",
  scope: (
    cmyk: (cyan: pos(any), magenta: pos(any), yellow: pos(any), key: pos(any), color: pos(any)) => any,
    components: (self: pos(any), alpha: named(any)) => any,
    darken: (self: pos(any), factor: pos(any)) => any,
    desaturate: (self: pos(any), factor: pos(any)) => any,
    hsl: (hue: pos(any), saturation: pos(any), lightness: pos(any), alpha: pos(any), color: pos(any)) => any,
    hsv: (hue: pos(any), saturation: pos(any), value: pos(any), alpha: pos(any), color: pos(any)) => any,
    lighten: (self: pos(any), factor: pos(any)) => any,
    linear-rgb: (red: pos(any), green: pos(any), blue: pos(any), alpha: pos(any), color: pos(any)) => any,
    luma: (lightness: pos(any), alpha: pos(any), color: pos(any)) => any,
    mix: (colors: rest(any), space: named(any)) => any,
    negate: (self: pos(any), space: named(any)) => any,
    oklab: (lightness: pos(any), a: pos(any), b: pos(any), alpha: pos(any), color: pos(any)) => any,
    oklch: (lightness: pos(any), chroma: pos(any), hue: pos(any), alpha: pos(any), color: pos(any)) => any,
    opacify: (self: pos(any), scale: pos(any)) => any,
    rgb: (red: pos(any), green: pos(any), blue: pos(any), alpha: pos(any), hex: pos(any), color: pos(any)) => any,
    rotate: (self: pos(any), angle: pos(any), space: named(any)) => any,
    saturate: (self: pos(any), factor: pos(any)) => any,
    space: (self: pos(any)) => any,
    to-hex: (self: pos(any)) => any,
    transparentize: (self: pos(any), scale: pos(any)) => any,
  ),
);

#let content = rec(
  name: "content",
  scope: (
    at: (self: pos(any), field: pos(any), default: named(any)) => any,
    fields: (self: pos(any)) => any,
    func: (self: pos(any)) => any,
    has: (self: pos(any), field: pos(any)) => any,
    location: (self: pos(any)) => any,
  ),
);

#let counter = rec(
  name: "counter",
  scope: (
    at: (self: pos(any), selector: pos(any)) => any,
    display: (self: pos(any), numbering: pos(any), both: named(any)) => any,
    final: (self: pos(any)) => any,
    get: (self: pos(any)) => any,
    step: (self: pos(any), level: named(any)) => any,
    update: (self: pos(any), update: pos(any)) => any,
  ),
);

#let datetime = rec(
  name: "datetime",
  scope: (
    day: (self: pos(any)) => any,
    display: (self: pos(any), pattern: pos(any)) => any,
    hour: (self: pos(any)) => any,
    minute: (self: pos(any)) => any,
    month: (self: pos(any)) => any,
    ordinal: (self: pos(any)) => any,
    second: (self: pos(any)) => any,
    today: (offset: named(any)) => any,
    weekday: (self: pos(any)) => any,
    year: (self: pos(any)) => any,
  ),
);

#let decimal = rec(
  name: "decimal",
  scope: (:),
);

#let dictionary = rec(
  name: "dictionary",
  scope: (
    at: (self: pos(any), key: pos(any), default: named(any)) => any,
    insert: (self: pos(any), key: pos(any), value: pos(any)) => any,
    keys: (self: pos(any)) => any,
    len: (self: pos(any)) => any,
    pairs: (self: pos(any)) => any,
    remove: (self: pos(any), key: pos(any), default: named(any)) => any,
    values: (self: pos(any)) => any,
  ),
);

#let direction = rec(
  name: "direction",
  scope: (
    axis: (self: pos(any)) => any,
    end: (self: pos(any)) => any,
    from: (side: pos(any)) => any,
    inv: (self: pos(any)) => any,
    sign: (self: pos(any)) => any,
    start: (self: pos(any)) => any,
    to: (side: pos(any)) => any,
  ),
);

#let duration = rec(
  name: "duration",
  scope: (
    days: (self: pos(any)) => any,
    hours: (self: pos(any)) => any,
    minutes: (self: pos(any)) => any,
    seconds: (self: pos(any)) => any,
    weeks: (self: pos(any)) => any,
  ),
);

#let float = rec(
  name: "float",
  scope: (
    from-bytes: (bytes: pos(any), endian: named(any)) => any,
    is-infinite: (self: pos(any)) => any,
    is-nan: (self: pos(any)) => any,
    signum: (self: pos(any)) => any,
    to-bytes: (self: pos(any), endian: named(any), size: named(any)) => any,
  ),
);

#let fraction = rec(
  name: "fraction",
  scope: (:),
);

#let function = rec(
  name: "function",
  scope: (
    where: (self: pos(any), fields: rest(any)) => any,
    with: (self: pos(any), arguments: rest(any)) => any,
  ),
);

#let gradient = rec(
  name: "gradient",
  scope: (
    angle: (self: pos(any)) => any,
    center: (self: pos(any)) => any,
    conic: (stops: rest(any), angle: named(any), space: named(any), relative: named(any), center: named(any)) => any,
    focal-center: (self: pos(any)) => any,
    focal-radius: (self: pos(any)) => any,
    kind: (self: pos(any)) => any,
    linear: (stops: rest(any), space: named(any), relative: named(any), dir: pos(any), angle: pos(any)) => any,
    radial: (stops: rest(any), space: named(any), relative: named(any), center: named(any), radius: named(any), focal-center: named(any), focal-radius: named(any)) => any,
    radius: (self: pos(any)) => any,
    relative: (self: pos(any)) => any,
    repeat: (self: pos(any), repetitions: pos(any), mirror: named(any)) => any,
    sample: (self: pos(any), t: pos(any)) => any,
    samples: (self: pos(any), ts: rest(any)) => any,
    sharp: (self: pos(any), steps: pos(any), smoothness: named(any)) => any,
    space: (self: pos(any)) => any,
    stops: (self: pos(any)) => any,
  ),
);

#let int = rec(
  name: "integer",
  scope: (
    bit-and: (self: pos(any), rhs: pos(any)) => any,
    bit-lshift: (self: pos(any), shift: pos(any)) => any,
    bit-not: (self: pos(any)) => any,
    bit-or: (self: pos(any), rhs: pos(any)) => any,
    bit-rshift: (self: pos(any), shift: pos(any), logical: named(any)) => any,
    bit-xor: (self: pos(any), rhs: pos(any)) => any,
    from-bytes: (bytes: pos(any), endian: named(any), signed: named(any)) => any,
    signum: (self: pos(any)) => any,
    to-bytes: (self: pos(any), endian: named(any), size: named(any)) => any,
  ),
);

#let label = rec(
  name: "label",
  scope: (:),
);

#let length = rec(
  name: "length",
  scope: (
    cm: (self: pos(any)) => any,
    inches: (self: pos(any)) => any,
    mm: (self: pos(any)) => any,
    pt: (self: pos(any)) => any,
    to-absolute: (self: pos(any)) => any,
  ),
);

#let location = rec(
  name: "location",
  scope: (
    page: (self: pos(any)) => any,
    page-numbering: (self: pos(any)) => any,
    position: (self: pos(any)) => any,
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
    after: (self: pos(any), start: pos(any), inclusive: named(any)) => any,
    "and": (self: pos(any), others: rest(any)) => any,
    before: (self: pos(any), end: pos(any), inclusive: named(any)) => any,
    "or": (self: pos(any), others: rest(any)) => any,
  ),
);

#let state = rec(
  name: "state",
  scope: (
    at: (self: pos(any), selector: pos(any)) => any,
    final: (self: pos(any)) => any,
    get: (self: pos(any)) => any,
    update: (self: pos(any), update: pos(any)) => any,
  ),
);

#let str = rec(
  name: "string",
  scope: (
    at: (self: pos(any), index: pos(any), default: named(any)) => any,
    clusters: (self: pos(any)) => any,
    codepoints: (self: pos(any)) => any,
    contains: (self: pos(any), pattern: pos(any)) => any,
    ends-with: (self: pos(any), pattern: pos(any)) => any,
    find: (self: pos(any), pattern: pos(any)) => any,
    first: (self: pos(any), default: named(any)) => any,
    from-unicode: (value: pos(any)) => any,
    last: (self: pos(any), default: named(any)) => any,
    len: (self: pos(any)) => any,
    match: (self: pos(any), pattern: pos(any)) => any,
    matches: (self: pos(any), pattern: pos(any)) => any,
    normalize: (self: pos(any), form: named(any)) => any,
    position: (self: pos(any), pattern: pos(any)) => any,
    replace: (self: pos(any), pattern: pos(any), replacement: pos(any), count: named(any)) => any,
    rev: (self: pos(any)) => any,
    slice: (self: pos(any), start: pos(any), end: pos(any), count: named(any)) => any,
    split: (self: pos(any), pattern: pos(any)) => any,
    starts-with: (self: pos(any), pattern: pos(any)) => any,
    to-unicode: (character: pos(any)) => any,
    trim: (self: pos(any), pattern: pos(any), at: named(any), repeat: named(any)) => any,
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
    at: (self: pos(any), index: pos(any)) => any,
  ),
);

// Backwards-compatible generic helpers used by existing fixtures.
#let array-type(V: any) = array;
#let dict-type(V: any) = dictionary;
#let str-type = str;
