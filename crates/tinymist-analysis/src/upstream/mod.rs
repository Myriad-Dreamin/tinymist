//! Functions from typst-ide

use std::{collections::HashMap, fmt::Write, sync::LazyLock};

use comemo::Tracked;
use ecow::{eco_format, EcoString};
use serde::Deserialize;
use serde_yaml as yaml;
use typst::{
    diag::{bail, StrResult},
    foundations::{Binding, Content, Func, Module, Type, Value},
    introspection::MetadataElem,
    syntax::Span,
    text::{FontInfo, FontStyle},
    Category, Library, World,
};

mod tooltip;
pub use tooltip::*;

/// Extract the first sentence of plain text of a piece of documentation.
///
/// Removes Markdown formatting.
pub fn plain_docs_sentence(docs: &str) -> EcoString {
    crate::log_debug_ct!("plain docs {docs:?}");
    let docs = docs.replace("```example", "```typ");
    let mut scanner = unscanny::Scanner::new(&docs);
    let mut output = EcoString::new();
    let mut link = false;
    while let Some(ch) = scanner.eat() {
        match ch {
            '`' => {
                let mut raw = scanner.eat_until('`');
                if (raw.starts_with('{') && raw.ends_with('}'))
                    || (raw.starts_with('[') && raw.ends_with(']'))
                {
                    raw = &raw[1..raw.len() - 1];
                }

                scanner.eat();
                output.push('`');
                output.push_str(raw);
                output.push('`');
            }
            '[' => {
                link = true;
                output.push('[');
            }
            ']' if link => {
                output.push(']');
                let cursor = scanner.cursor();
                if scanner.eat_if('(') {
                    scanner.eat_until(')');
                    let link_content = scanner.from(cursor + 1);
                    scanner.eat();

                    crate::log_debug_ct!("Intra Link: {link_content}");
                    let link = resolve(link_content, "https://typst.app/docs/").ok();
                    let link = link.unwrap_or_else(|| {
                        log::warn!("Failed to resolve link: {link_content}");
                        "https://typst.app/docs/404.html".to_string()
                    });

                    output.push('(');
                    output.push_str(&link);
                    output.push(')');
                } else if scanner.eat_if('[') {
                    scanner.eat_until(']');
                    scanner.eat();
                    output.push_str(scanner.from(cursor));
                }
                link = false
            }
            // '*' | '_' => {}
            // '.' => {
            //     output.push('.');
            //     break;
            // }
            _ => output.push(ch),
        }
    }

    output
}

/// Data about a collection of functions.
#[derive(Debug, Clone, Deserialize)]
struct GroupData {
    name: EcoString,
    // title: EcoString,
    category: EcoString,
    #[serde(default)]
    path: Vec<EcoString>,
    #[serde(default)]
    filter: Vec<EcoString>,
    // details: EcoString,
}

impl GroupData {
    fn module(&self) -> &'static Module {
        let mut focus = &LIBRARY.global;
        for path in &self.path {
            focus = get_module(focus, path).unwrap();
        }
        focus
    }
}

static GROUPS: LazyLock<Vec<GroupData>> = LazyLock::new(|| {
    let mut groups: Vec<GroupData> = yaml::from_str(include_str!("groups.yml")).unwrap();
    for group in &mut groups {
        if group.filter.is_empty() {
            group.filter = group
                .module()
                .scope()
                .iter()
                .filter(|(_, v)| matches!(v.read(), Value::Func(_)))
                .map(|(k, _)| k.clone())
                .collect();
        }
    }
    groups
});

/// Resolve an intra-doc link.
pub fn resolve(link: &str, base: &str) -> StrResult<String> {
    if link.starts_with('#') || link.starts_with("http") {
        return Ok(link.to_string());
    }

    let (head, tail) = split_link(link)?;
    let mut route = match resolve_known(head, base) {
        Some(route) => route,
        None => resolve_definition(head, base)?,
    };

    if !tail.is_empty() {
        route.push('/');
        route.push_str(tail);
    }

    if !route.contains(['#', '?']) && !route.ends_with('/') {
        route.push('/');
    }

    Ok(route)
}

/// Split a link at the first slash.
fn split_link(link: &str) -> StrResult<(&str, &str)> {
    let first = link.split('/').next().unwrap_or(link);
    let rest = link[first.len()..].trim_start_matches('/');
    Ok((first, rest))
}

/// Resolve a `$` link head to a known destination.
fn resolve_known(head: &str, base: &str) -> Option<String> {
    Some(match head {
        "$tutorial" => format!("{base}tutorial"),
        "$reference" => format!("{base}reference"),
        "$category" => format!("{base}reference"),
        "$syntax" => format!("{base}reference/syntax"),
        "$styling" => format!("{base}reference/styling"),
        "$scripting" => format!("{base}reference/scripting"),
        "$context" => format!("{base}reference/context"),
        "$guides" => format!("{base}guides"),
        "$changelog" => format!("{base}changelog"),
        "$community" => format!("{base}community"),
        "$universe" => "https://typst.app/universe".into(),
        _ => return None,
    })
}

static LIBRARY: LazyLock<Library> = LazyLock::new(Library::default);

/// Extract a module from another module.
#[track_caller]
fn get_module<'a>(parent: &'a Module, name: &str) -> StrResult<&'a Module> {
    match parent.scope().get(name).map(|x| x.read()) {
        Some(Value::Module(module)) => Ok(module),
        _ => bail!("module doesn't contain module `{name}`"),
    }
}

/// Resolve a `$` link to a global definition.
fn resolve_definition(head: &str, base: &str) -> StrResult<String> {
    let mut parts = head.trim_start_matches('$').split('.').peekable();
    let mut focus = &LIBRARY.global;
    let mut category = None;

    while let Some(name) = parts.peek() {
        if category.is_none() {
            category = focus.scope().get(name).and_then(Binding::category);
        }
        let Ok(module) = get_module(focus, name) else {
            break;
        };
        focus = module;
        parts.next();
    }

    let Some(category) = category else {
        bail!("{head} has no category")
    };

    let name = parts.next().ok_or("link is missing first part")?;
    let value = focus.field(name, ())?;

    // Handle grouped functions.
    if let Some(group) = GROUPS.iter().find(|group| {
        group.category == category.name() && group.filter.iter().any(|func| func == name)
    }) {
        let mut route = format!(
            "{}reference/{}/{}/#functions-{}",
            base, group.category, group.name, name
        );
        if let Some(param) = parts.next() {
            route.push('-');
            route.push_str(param);
        }
        return Ok(route);
    }

    let mut route = format!("{}reference/{}/{name}", base, category.name());
    if let Some(next) = parts.next() {
        if let Ok(field) = value.field(next, ()) {
            route.push_str("/#definitions-");
            route.push_str(next);
            if let Some(next) = parts.next() {
                if field
                    .cast::<Func>()
                    .is_ok_and(|func| func.param(next).is_some())
                {
                    route.push('-');
                    route.push_str(next);
                }
            }
        } else if value
            .clone()
            .cast::<Func>()
            .is_ok_and(|func| func.param(next).is_some())
        {
            route.push_str("/#parameters-");
            route.push_str(next);
        } else {
            bail!("field {next} not found");
        }
    }

    Ok(route)
}

#[allow(clippy::derived_hash_with_manual_eq)]
#[derive(Debug, Clone, Hash)]
enum CatKey {
    Func(Func),
    Type(Type),
}

impl PartialEq for CatKey {
    fn eq(&self, other: &Self) -> bool {
        use typst::foundations::func::Repr::*;
        match (self, other) {
            (CatKey::Func(a), CatKey::Func(b)) => match (a.inner(), b.inner()) {
                (Native(a), Native(b)) => a == b,
                (Element(a), Element(b)) => a == b,
                _ => false,
            },
            (CatKey::Type(a), CatKey::Type(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for CatKey {}

// todo: category of types
static ROUTE_MAPS: LazyLock<HashMap<CatKey, String>> = LazyLock::new(|| {
    // todo: this is a false positive for clippy on LazyHash
    #[allow(clippy::mutable_key_type)]
    let mut map = HashMap::new();
    let mut scope_to_finds = vec![
        (LIBRARY.global.scope(), None, None),
        (LIBRARY.math.scope(), None, None),
    ];
    while let Some((scope, parent_name, cat)) = scope_to_finds.pop() {
        for (name, bind) in scope.iter() {
            let cat = cat.or_else(|| bind.category());
            let name = urlify(name);
            match bind.read() {
                Value::Func(func) => {
                    if let Some(cat) = cat {
                        let Some(name) = func.name() else {
                            continue;
                        };

                        // Handle grouped functions.
                        if let Some(group) = GROUPS.iter().find(|group| {
                            group.category == cat.name()
                                && group.filter.iter().any(|func| func == name)
                        }) {
                            let route = format!(
                                "reference/{}/{}/#functions-{name}",
                                group.category, group.name
                            );
                            map.insert(CatKey::Func(func.clone()), route);
                            continue;
                        }

                        crate::log_debug_ct!("func: {func:?} -> {cat:?}");

                        let route = format_route(parent_name.as_deref(), name, &cat);

                        map.insert(CatKey::Func(func.clone()), route);
                    }
                    if let Some(s) = func.scope() {
                        scope_to_finds.push((s, Some(name), cat));
                    }
                }
                Value::Type(t) => {
                    if let Some(cat) = cat {
                        crate::log_debug_ct!("type: {t:?} -> {cat:?}");

                        let route = format_route(parent_name.as_deref(), &name, &cat);
                        map.insert(CatKey::Type(*t), route);
                    }
                    scope_to_finds.push((t.scope(), Some(name), cat));
                }
                Value::Module(module) => {
                    scope_to_finds.push((module.scope(), Some(name), cat));
                }
                _ => {}
            }
        }
    }
    map
});

fn format_route(parent_name: Option<&str>, name: &str, cat: &Category) -> String {
    match parent_name {
        Some(parent_name) if parent_name != cat.name() => {
            format!("reference/{}/{parent_name}/#definitions-{name}", cat.name())
        }
        Some(_) | None => format!("reference/{}/{name}/", cat.name()),
    }
}

/// Turn a title into an URL fragment.
pub(crate) fn urlify(title: &str) -> EcoString {
    title
        .chars()
        .map(|ch| ch.to_ascii_lowercase())
        .map(|ch| match ch {
            'a'..='z' | '0'..='9' => ch,
            _ => '-',
        })
        .collect()
}

/// Get the route of a value.
pub fn route_of_value(val: &Value) -> Option<&'static String> {
    // ROUTE_MAPS.get(&CatKey::Func(k.clone()))
    let key = match val {
        Value::Func(func) => CatKey::Func(func.clone()),
        Value::Type(ty) => CatKey::Type(*ty),
        _ => return None,
    };

    ROUTE_MAPS.get(&key)
}

/// Create a short description of a font family.
pub fn summarize_font_family<'a>(variants: impl Iterator<Item = &'a FontInfo>) -> EcoString {
    let mut infos: Vec<_> = variants.collect();
    infos.sort_by_key(|info: &&FontInfo| info.variant);

    let mut has_italic = false;
    let mut min_weight = u16::MAX;
    let mut max_weight = 0;
    for info in &infos {
        let weight = info.variant.weight.to_number();
        has_italic |= info.variant.style == FontStyle::Italic;
        min_weight = min_weight.min(weight);
        max_weight = min_weight.max(weight);
    }

    let count = infos.len();
    let mut detail = eco_format!("{count} variant{}.", if count == 1 { "" } else { "s" });

    if min_weight == max_weight {
        write!(detail, " Weight {min_weight}.").unwrap();
    } else {
        write!(detail, " Weights {min_weight}–{max_weight}.").unwrap();
    }

    if has_italic {
        detail.push_str(" Has italics.");
    }

    detail
}

/// Get the representation but truncated to a certain size.
pub fn truncated_repr_<const SZ_LIMIT: usize>(value: &Value) -> EcoString {
    use typst::foundations::Repr;

    let data: Option<Content> = value.clone().cast().ok();
    let metadata: Option<MetadataElem> = data.and_then(|content| content.unpack().ok());

    // todo: early truncation
    let repr = if let Some(metadata) = metadata {
        metadata.value.repr()
    } else {
        value.repr()
    };

    if repr.len() > SZ_LIMIT {
        eco_format!("[truncated-repr: {} bytes]", repr.len())
    } else {
        repr
    }
}

/// Get the representation but truncated to a certain size.
pub fn truncated_repr(value: &Value) -> EcoString {
    const _10MB: usize = 100 * 1024 * 1024;
    truncated_repr_::<_10MB>(value)
}

/// Run a function with a VM instance in the world
pub fn with_vm<T>(
    world: Tracked<dyn World + '_>,
    f: impl FnOnce(&mut typst_shim::eval::Vm) -> T,
) -> T {
    use comemo::Track;
    use typst::engine::*;
    use typst::foundations::*;
    use typst::introspection::*;
    use typst_shim::eval::*;

    let introspector = Introspector::default();
    let traced = Traced::default();
    let mut sink = Sink::new();
    let engine = Engine {
        routines: &typst::ROUTINES,
        world,
        route: Route::default(),
        introspector: introspector.track(),
        traced: traced.track(),
        sink: sink.track_mut(),
    };

    let context = Context::none();
    let mut vm = Vm::new(
        engine,
        context.track(),
        Scopes::new(Some(world.library())),
        Span::detached(),
    );

    f(&mut vm)
}

#[cfg(test)]
mod tests {
    use crate::upstream::ROUTE_MAPS;

    #[test]
    fn docs_test() {
        assert_eq!(
            "[citation](https://typst.app/docs/reference/model/cite/)",
            super::plain_docs_sentence("[citation]($cite)")
        );
        assert_eq!(
            "[citation][cite]",
            super::plain_docs_sentence("[citation][cite]")
        );
        assert_eq!(
            "[citation](https://typst.app/docs/reference/model/cite/)",
            super::plain_docs_sentence("[citation]($cite)")
        );
        assert_eq!(
            "[citation][cite][cite2]",
            super::plain_docs_sentence("[citation][cite][cite2]")
        );
        assert_eq!(
            "[citation][cite](test)[cite2]",
            super::plain_docs_sentence("[citation][cite](test)[cite2]")
        );
    }

    #[test]
    fn routes() {
        let access = |route: &String| format!("https://typst.app/docs/{route}");
        let mut values = ROUTE_MAPS.values().map(access).collect::<Vec<_>>();
        values.sort();

        insta::assert_snapshot!(values.as_slice().join("\n"), @r###"
        https://typst.app/docs/reference/data-loading/cbor/
        https://typst.app/docs/reference/data-loading/cbor/#definitions-decode
        https://typst.app/docs/reference/data-loading/cbor/#definitions-encode
        https://typst.app/docs/reference/data-loading/csv/
        https://typst.app/docs/reference/data-loading/csv/#definitions-decode
        https://typst.app/docs/reference/data-loading/json/
        https://typst.app/docs/reference/data-loading/json/#definitions-decode
        https://typst.app/docs/reference/data-loading/json/#definitions-encode
        https://typst.app/docs/reference/data-loading/read/
        https://typst.app/docs/reference/data-loading/toml/
        https://typst.app/docs/reference/data-loading/toml/#definitions-decode
        https://typst.app/docs/reference/data-loading/toml/#definitions-encode
        https://typst.app/docs/reference/data-loading/xml/
        https://typst.app/docs/reference/data-loading/xml/#definitions-decode
        https://typst.app/docs/reference/data-loading/yaml/
        https://typst.app/docs/reference/data-loading/yaml/#definitions-decode
        https://typst.app/docs/reference/data-loading/yaml/#definitions-encode
        https://typst.app/docs/reference/foundations/arguments/
        https://typst.app/docs/reference/foundations/arguments/#definitions-at
        https://typst.app/docs/reference/foundations/arguments/#definitions-named
        https://typst.app/docs/reference/foundations/arguments/#definitions-pos
        https://typst.app/docs/reference/foundations/array/
        https://typst.app/docs/reference/foundations/array/#definitions-all
        https://typst.app/docs/reference/foundations/array/#definitions-any
        https://typst.app/docs/reference/foundations/array/#definitions-at
        https://typst.app/docs/reference/foundations/array/#definitions-chunks
        https://typst.app/docs/reference/foundations/array/#definitions-contains
        https://typst.app/docs/reference/foundations/array/#definitions-dedup
        https://typst.app/docs/reference/foundations/array/#definitions-enumerate
        https://typst.app/docs/reference/foundations/array/#definitions-filter
        https://typst.app/docs/reference/foundations/array/#definitions-find
        https://typst.app/docs/reference/foundations/array/#definitions-first
        https://typst.app/docs/reference/foundations/array/#definitions-flatten
        https://typst.app/docs/reference/foundations/array/#definitions-fold
        https://typst.app/docs/reference/foundations/array/#definitions-insert
        https://typst.app/docs/reference/foundations/array/#definitions-intersperse
        https://typst.app/docs/reference/foundations/array/#definitions-join
        https://typst.app/docs/reference/foundations/array/#definitions-last
        https://typst.app/docs/reference/foundations/array/#definitions-len
        https://typst.app/docs/reference/foundations/array/#definitions-map
        https://typst.app/docs/reference/foundations/array/#definitions-pop
        https://typst.app/docs/reference/foundations/array/#definitions-position
        https://typst.app/docs/reference/foundations/array/#definitions-product
        https://typst.app/docs/reference/foundations/array/#definitions-push
        https://typst.app/docs/reference/foundations/array/#definitions-range
        https://typst.app/docs/reference/foundations/array/#definitions-reduce
        https://typst.app/docs/reference/foundations/array/#definitions-remove
        https://typst.app/docs/reference/foundations/array/#definitions-rev
        https://typst.app/docs/reference/foundations/array/#definitions-slice
        https://typst.app/docs/reference/foundations/array/#definitions-sorted
        https://typst.app/docs/reference/foundations/array/#definitions-split
        https://typst.app/docs/reference/foundations/array/#definitions-sum
        https://typst.app/docs/reference/foundations/array/#definitions-to-dict
        https://typst.app/docs/reference/foundations/array/#definitions-windows
        https://typst.app/docs/reference/foundations/array/#definitions-zip
        https://typst.app/docs/reference/foundations/assert/
        https://typst.app/docs/reference/foundations/assert/#definitions-eq
        https://typst.app/docs/reference/foundations/assert/#definitions-ne
        https://typst.app/docs/reference/foundations/bool/
        https://typst.app/docs/reference/foundations/bytes/
        https://typst.app/docs/reference/foundations/bytes/#definitions-at
        https://typst.app/docs/reference/foundations/bytes/#definitions-len
        https://typst.app/docs/reference/foundations/bytes/#definitions-slice
        https://typst.app/docs/reference/foundations/calc/#functions-abs
        https://typst.app/docs/reference/foundations/calc/#functions-acos
        https://typst.app/docs/reference/foundations/calc/#functions-asin
        https://typst.app/docs/reference/foundations/calc/#functions-atan
        https://typst.app/docs/reference/foundations/calc/#functions-atan2
        https://typst.app/docs/reference/foundations/calc/#functions-binom
        https://typst.app/docs/reference/foundations/calc/#functions-ceil
        https://typst.app/docs/reference/foundations/calc/#functions-clamp
        https://typst.app/docs/reference/foundations/calc/#functions-cos
        https://typst.app/docs/reference/foundations/calc/#functions-cosh
        https://typst.app/docs/reference/foundations/calc/#functions-div-euclid
        https://typst.app/docs/reference/foundations/calc/#functions-even
        https://typst.app/docs/reference/foundations/calc/#functions-exp
        https://typst.app/docs/reference/foundations/calc/#functions-fact
        https://typst.app/docs/reference/foundations/calc/#functions-floor
        https://typst.app/docs/reference/foundations/calc/#functions-fract
        https://typst.app/docs/reference/foundations/calc/#functions-gcd
        https://typst.app/docs/reference/foundations/calc/#functions-lcm
        https://typst.app/docs/reference/foundations/calc/#functions-ln
        https://typst.app/docs/reference/foundations/calc/#functions-log
        https://typst.app/docs/reference/foundations/calc/#functions-max
        https://typst.app/docs/reference/foundations/calc/#functions-min
        https://typst.app/docs/reference/foundations/calc/#functions-norm
        https://typst.app/docs/reference/foundations/calc/#functions-odd
        https://typst.app/docs/reference/foundations/calc/#functions-perm
        https://typst.app/docs/reference/foundations/calc/#functions-pow
        https://typst.app/docs/reference/foundations/calc/#functions-quo
        https://typst.app/docs/reference/foundations/calc/#functions-rem
        https://typst.app/docs/reference/foundations/calc/#functions-rem-euclid
        https://typst.app/docs/reference/foundations/calc/#functions-root
        https://typst.app/docs/reference/foundations/calc/#functions-round
        https://typst.app/docs/reference/foundations/calc/#functions-sin
        https://typst.app/docs/reference/foundations/calc/#functions-sinh
        https://typst.app/docs/reference/foundations/calc/#functions-sqrt
        https://typst.app/docs/reference/foundations/calc/#functions-tan
        https://typst.app/docs/reference/foundations/calc/#functions-tanh
        https://typst.app/docs/reference/foundations/calc/#functions-trunc
        https://typst.app/docs/reference/foundations/content/
        https://typst.app/docs/reference/foundations/content/#definitions-at
        https://typst.app/docs/reference/foundations/content/#definitions-fields
        https://typst.app/docs/reference/foundations/content/#definitions-func
        https://typst.app/docs/reference/foundations/content/#definitions-has
        https://typst.app/docs/reference/foundations/content/#definitions-location
        https://typst.app/docs/reference/foundations/datetime/
        https://typst.app/docs/reference/foundations/datetime/#definitions-day
        https://typst.app/docs/reference/foundations/datetime/#definitions-display
        https://typst.app/docs/reference/foundations/datetime/#definitions-hour
        https://typst.app/docs/reference/foundations/datetime/#definitions-minute
        https://typst.app/docs/reference/foundations/datetime/#definitions-month
        https://typst.app/docs/reference/foundations/datetime/#definitions-ordinal
        https://typst.app/docs/reference/foundations/datetime/#definitions-second
        https://typst.app/docs/reference/foundations/datetime/#definitions-today
        https://typst.app/docs/reference/foundations/datetime/#definitions-weekday
        https://typst.app/docs/reference/foundations/datetime/#definitions-year
        https://typst.app/docs/reference/foundations/decimal/
        https://typst.app/docs/reference/foundations/dictionary/
        https://typst.app/docs/reference/foundations/dictionary/#definitions-at
        https://typst.app/docs/reference/foundations/dictionary/#definitions-insert
        https://typst.app/docs/reference/foundations/dictionary/#definitions-keys
        https://typst.app/docs/reference/foundations/dictionary/#definitions-len
        https://typst.app/docs/reference/foundations/dictionary/#definitions-pairs
        https://typst.app/docs/reference/foundations/dictionary/#definitions-remove
        https://typst.app/docs/reference/foundations/dictionary/#definitions-values
        https://typst.app/docs/reference/foundations/duration/
        https://typst.app/docs/reference/foundations/duration/#definitions-days
        https://typst.app/docs/reference/foundations/duration/#definitions-hours
        https://typst.app/docs/reference/foundations/duration/#definitions-minutes
        https://typst.app/docs/reference/foundations/duration/#definitions-seconds
        https://typst.app/docs/reference/foundations/duration/#definitions-weeks
        https://typst.app/docs/reference/foundations/eval/
        https://typst.app/docs/reference/foundations/float/
        https://typst.app/docs/reference/foundations/float/#definitions-from-bytes
        https://typst.app/docs/reference/foundations/float/#definitions-is-infinite
        https://typst.app/docs/reference/foundations/float/#definitions-is-nan
        https://typst.app/docs/reference/foundations/float/#definitions-signum
        https://typst.app/docs/reference/foundations/float/#definitions-to-bytes
        https://typst.app/docs/reference/foundations/function/
        https://typst.app/docs/reference/foundations/function/#definitions-where
        https://typst.app/docs/reference/foundations/function/#definitions-with
        https://typst.app/docs/reference/foundations/int/
        https://typst.app/docs/reference/foundations/int/#definitions-bit-and
        https://typst.app/docs/reference/foundations/int/#definitions-bit-lshift
        https://typst.app/docs/reference/foundations/int/#definitions-bit-not
        https://typst.app/docs/reference/foundations/int/#definitions-bit-or
        https://typst.app/docs/reference/foundations/int/#definitions-bit-rshift
        https://typst.app/docs/reference/foundations/int/#definitions-bit-xor
        https://typst.app/docs/reference/foundations/int/#definitions-from-bytes
        https://typst.app/docs/reference/foundations/int/#definitions-signum
        https://typst.app/docs/reference/foundations/int/#definitions-to-bytes
        https://typst.app/docs/reference/foundations/label/
        https://typst.app/docs/reference/foundations/module/
        https://typst.app/docs/reference/foundations/panic/
        https://typst.app/docs/reference/foundations/plugin/
        https://typst.app/docs/reference/foundations/plugin/#definitions-transition
        https://typst.app/docs/reference/foundations/regex/
        https://typst.app/docs/reference/foundations/repr/
        https://typst.app/docs/reference/foundations/selector/
        https://typst.app/docs/reference/foundations/selector/#definitions-after
        https://typst.app/docs/reference/foundations/selector/#definitions-and
        https://typst.app/docs/reference/foundations/selector/#definitions-before
        https://typst.app/docs/reference/foundations/selector/#definitions-or
        https://typst.app/docs/reference/foundations/str/
        https://typst.app/docs/reference/foundations/str/#definitions-at
        https://typst.app/docs/reference/foundations/str/#definitions-clusters
        https://typst.app/docs/reference/foundations/str/#definitions-codepoints
        https://typst.app/docs/reference/foundations/str/#definitions-contains
        https://typst.app/docs/reference/foundations/str/#definitions-ends-with
        https://typst.app/docs/reference/foundations/str/#definitions-find
        https://typst.app/docs/reference/foundations/str/#definitions-first
        https://typst.app/docs/reference/foundations/str/#definitions-from-unicode
        https://typst.app/docs/reference/foundations/str/#definitions-last
        https://typst.app/docs/reference/foundations/str/#definitions-len
        https://typst.app/docs/reference/foundations/str/#definitions-match
        https://typst.app/docs/reference/foundations/str/#definitions-matches
        https://typst.app/docs/reference/foundations/str/#definitions-position
        https://typst.app/docs/reference/foundations/str/#definitions-replace
        https://typst.app/docs/reference/foundations/str/#definitions-rev
        https://typst.app/docs/reference/foundations/str/#definitions-slice
        https://typst.app/docs/reference/foundations/str/#definitions-split
        https://typst.app/docs/reference/foundations/str/#definitions-starts-with
        https://typst.app/docs/reference/foundations/str/#definitions-to-unicode
        https://typst.app/docs/reference/foundations/str/#definitions-trim
        https://typst.app/docs/reference/foundations/symbol/
        https://typst.app/docs/reference/foundations/type/
        https://typst.app/docs/reference/foundations/version/
        https://typst.app/docs/reference/foundations/version/#definitions-at
        https://typst.app/docs/reference/introspection/counter/
        https://typst.app/docs/reference/introspection/counter/#definitions-at
        https://typst.app/docs/reference/introspection/counter/#definitions-display
        https://typst.app/docs/reference/introspection/counter/#definitions-final
        https://typst.app/docs/reference/introspection/counter/#definitions-get
        https://typst.app/docs/reference/introspection/counter/#definitions-step
        https://typst.app/docs/reference/introspection/counter/#definitions-update
        https://typst.app/docs/reference/introspection/here/
        https://typst.app/docs/reference/introspection/locate/
        https://typst.app/docs/reference/introspection/location/
        https://typst.app/docs/reference/introspection/location/#definitions-page
        https://typst.app/docs/reference/introspection/location/#definitions-page-numbering
        https://typst.app/docs/reference/introspection/location/#definitions-position
        https://typst.app/docs/reference/introspection/metadata/
        https://typst.app/docs/reference/introspection/query/
        https://typst.app/docs/reference/introspection/state/
        https://typst.app/docs/reference/introspection/state/#definitions-at
        https://typst.app/docs/reference/introspection/state/#definitions-final
        https://typst.app/docs/reference/introspection/state/#definitions-get
        https://typst.app/docs/reference/introspection/state/#definitions-update
        https://typst.app/docs/reference/layout/align/
        https://typst.app/docs/reference/layout/alignment/
        https://typst.app/docs/reference/layout/alignment/#definitions-axis
        https://typst.app/docs/reference/layout/alignment/#definitions-inv
        https://typst.app/docs/reference/layout/angle/
        https://typst.app/docs/reference/layout/angle/#definitions-deg
        https://typst.app/docs/reference/layout/angle/#definitions-rad
        https://typst.app/docs/reference/layout/block/
        https://typst.app/docs/reference/layout/box/
        https://typst.app/docs/reference/layout/colbreak/
        https://typst.app/docs/reference/layout/columns/
        https://typst.app/docs/reference/layout/direction/
        https://typst.app/docs/reference/layout/direction/#definitions-axis
        https://typst.app/docs/reference/layout/direction/#definitions-end
        https://typst.app/docs/reference/layout/direction/#definitions-inv
        https://typst.app/docs/reference/layout/direction/#definitions-start
        https://typst.app/docs/reference/layout/fraction/
        https://typst.app/docs/reference/layout/grid/
        https://typst.app/docs/reference/layout/grid/#definitions-cell
        https://typst.app/docs/reference/layout/grid/#definitions-footer
        https://typst.app/docs/reference/layout/grid/#definitions-header
        https://typst.app/docs/reference/layout/grid/#definitions-hline
        https://typst.app/docs/reference/layout/grid/#definitions-vline
        https://typst.app/docs/reference/layout/h/
        https://typst.app/docs/reference/layout/hide/
        https://typst.app/docs/reference/layout/layout/
        https://typst.app/docs/reference/layout/length/
        https://typst.app/docs/reference/layout/length/#definitions-cm
        https://typst.app/docs/reference/layout/length/#definitions-inches
        https://typst.app/docs/reference/layout/length/#definitions-mm
        https://typst.app/docs/reference/layout/length/#definitions-pt
        https://typst.app/docs/reference/layout/length/#definitions-to-absolute
        https://typst.app/docs/reference/layout/measure/
        https://typst.app/docs/reference/layout/move/
        https://typst.app/docs/reference/layout/pad/
        https://typst.app/docs/reference/layout/page/
        https://typst.app/docs/reference/layout/pagebreak/
        https://typst.app/docs/reference/layout/place/
        https://typst.app/docs/reference/layout/place/#definitions-flush
        https://typst.app/docs/reference/layout/ratio/
        https://typst.app/docs/reference/layout/relative/
        https://typst.app/docs/reference/layout/repeat/
        https://typst.app/docs/reference/layout/rotate/
        https://typst.app/docs/reference/layout/scale/
        https://typst.app/docs/reference/layout/skew/
        https://typst.app/docs/reference/layout/stack/
        https://typst.app/docs/reference/layout/v/
        https://typst.app/docs/reference/math/accent/
        https://typst.app/docs/reference/math/attach/#functions-attach
        https://typst.app/docs/reference/math/attach/#functions-limits
        https://typst.app/docs/reference/math/attach/#functions-scripts
        https://typst.app/docs/reference/math/binom/
        https://typst.app/docs/reference/math/cancel/
        https://typst.app/docs/reference/math/cases/
        https://typst.app/docs/reference/math/class/
        https://typst.app/docs/reference/math/equation/
        https://typst.app/docs/reference/math/frac/
        https://typst.app/docs/reference/math/lr/#functions-abs
        https://typst.app/docs/reference/math/lr/#functions-lr
        https://typst.app/docs/reference/math/lr/#functions-mid
        https://typst.app/docs/reference/math/lr/#functions-norm
        https://typst.app/docs/reference/math/lr/#functions-round
        https://typst.app/docs/reference/math/mat/
        https://typst.app/docs/reference/math/op/
        https://typst.app/docs/reference/math/primes/
        https://typst.app/docs/reference/math/roots/#functions-root
        https://typst.app/docs/reference/math/roots/#functions-sqrt
        https://typst.app/docs/reference/math/sizes/#functions-display
        https://typst.app/docs/reference/math/sizes/#functions-inline
        https://typst.app/docs/reference/math/sizes/#functions-script
        https://typst.app/docs/reference/math/sizes/#functions-sscript
        https://typst.app/docs/reference/math/stretch/
        https://typst.app/docs/reference/math/styles/#functions-bold
        https://typst.app/docs/reference/math/styles/#functions-italic
        https://typst.app/docs/reference/math/styles/#functions-upright
        https://typst.app/docs/reference/math/text/
        https://typst.app/docs/reference/math/underover/#functions-overbrace
        https://typst.app/docs/reference/math/underover/#functions-overbracket
        https://typst.app/docs/reference/math/underover/#functions-overline
        https://typst.app/docs/reference/math/underover/#functions-overparen
        https://typst.app/docs/reference/math/underover/#functions-overshell
        https://typst.app/docs/reference/math/underover/#functions-underbrace
        https://typst.app/docs/reference/math/underover/#functions-underbracket
        https://typst.app/docs/reference/math/underover/#functions-underline
        https://typst.app/docs/reference/math/underover/#functions-underparen
        https://typst.app/docs/reference/math/underover/#functions-undershell
        https://typst.app/docs/reference/math/variants/#functions-bb
        https://typst.app/docs/reference/math/variants/#functions-cal
        https://typst.app/docs/reference/math/variants/#functions-frak
        https://typst.app/docs/reference/math/variants/#functions-mono
        https://typst.app/docs/reference/math/variants/#functions-sans
        https://typst.app/docs/reference/math/variants/#functions-serif
        https://typst.app/docs/reference/math/vec/
        https://typst.app/docs/reference/model/bibliography/
        https://typst.app/docs/reference/model/cite/
        https://typst.app/docs/reference/model/document/
        https://typst.app/docs/reference/model/emph/
        https://typst.app/docs/reference/model/entry/#definitions-body
        https://typst.app/docs/reference/model/entry/#definitions-indented
        https://typst.app/docs/reference/model/entry/#definitions-inner
        https://typst.app/docs/reference/model/entry/#definitions-page
        https://typst.app/docs/reference/model/entry/#definitions-prefix
        https://typst.app/docs/reference/model/enum/
        https://typst.app/docs/reference/model/enum/#definitions-item
        https://typst.app/docs/reference/model/figure/
        https://typst.app/docs/reference/model/figure/#definitions-caption
        https://typst.app/docs/reference/model/footnote/
        https://typst.app/docs/reference/model/footnote/#definitions-entry
        https://typst.app/docs/reference/model/heading/
        https://typst.app/docs/reference/model/link/
        https://typst.app/docs/reference/model/list/
        https://typst.app/docs/reference/model/list/#definitions-item
        https://typst.app/docs/reference/model/numbering/
        https://typst.app/docs/reference/model/outline/
        https://typst.app/docs/reference/model/outline/#definitions-entry
        https://typst.app/docs/reference/model/par/
        https://typst.app/docs/reference/model/par/#definitions-line
        https://typst.app/docs/reference/model/parbreak/
        https://typst.app/docs/reference/model/quote/
        https://typst.app/docs/reference/model/ref/
        https://typst.app/docs/reference/model/strong/
        https://typst.app/docs/reference/model/table/
        https://typst.app/docs/reference/model/table/#definitions-cell
        https://typst.app/docs/reference/model/table/#definitions-footer
        https://typst.app/docs/reference/model/table/#definitions-header
        https://typst.app/docs/reference/model/table/#definitions-hline
        https://typst.app/docs/reference/model/table/#definitions-vline
        https://typst.app/docs/reference/model/terms/
        https://typst.app/docs/reference/model/terms/#definitions-item
        https://typst.app/docs/reference/pdf/embed/
        https://typst.app/docs/reference/text/highlight/
        https://typst.app/docs/reference/text/linebreak/
        https://typst.app/docs/reference/text/lorem/
        https://typst.app/docs/reference/text/lower/
        https://typst.app/docs/reference/text/overline/
        https://typst.app/docs/reference/text/raw/
        https://typst.app/docs/reference/text/raw/#definitions-line
        https://typst.app/docs/reference/text/smallcaps/
        https://typst.app/docs/reference/text/smartquote/
        https://typst.app/docs/reference/text/strike/
        https://typst.app/docs/reference/text/sub/
        https://typst.app/docs/reference/text/super/
        https://typst.app/docs/reference/text/underline/
        https://typst.app/docs/reference/text/upper/
        https://typst.app/docs/reference/visualize/circle/
        https://typst.app/docs/reference/visualize/color/
        https://typst.app/docs/reference/visualize/color/#definitions-cmyk
        https://typst.app/docs/reference/visualize/color/#definitions-components
        https://typst.app/docs/reference/visualize/color/#definitions-darken
        https://typst.app/docs/reference/visualize/color/#definitions-desaturate
        https://typst.app/docs/reference/visualize/color/#definitions-hsl
        https://typst.app/docs/reference/visualize/color/#definitions-hsv
        https://typst.app/docs/reference/visualize/color/#definitions-lighten
        https://typst.app/docs/reference/visualize/color/#definitions-linear-rgb
        https://typst.app/docs/reference/visualize/color/#definitions-luma
        https://typst.app/docs/reference/visualize/color/#definitions-mix
        https://typst.app/docs/reference/visualize/color/#definitions-negate
        https://typst.app/docs/reference/visualize/color/#definitions-oklab
        https://typst.app/docs/reference/visualize/color/#definitions-oklch
        https://typst.app/docs/reference/visualize/color/#definitions-opacify
        https://typst.app/docs/reference/visualize/color/#definitions-rgb
        https://typst.app/docs/reference/visualize/color/#definitions-rotate
        https://typst.app/docs/reference/visualize/color/#definitions-saturate
        https://typst.app/docs/reference/visualize/color/#definitions-space
        https://typst.app/docs/reference/visualize/color/#definitions-to-hex
        https://typst.app/docs/reference/visualize/color/#definitions-transparentize
        https://typst.app/docs/reference/visualize/curve/
        https://typst.app/docs/reference/visualize/curve/#definitions-close
        https://typst.app/docs/reference/visualize/curve/#definitions-cubic
        https://typst.app/docs/reference/visualize/curve/#definitions-line
        https://typst.app/docs/reference/visualize/curve/#definitions-move
        https://typst.app/docs/reference/visualize/curve/#definitions-quad
        https://typst.app/docs/reference/visualize/ellipse/
        https://typst.app/docs/reference/visualize/gradient/
        https://typst.app/docs/reference/visualize/gradient/#definitions-angle
        https://typst.app/docs/reference/visualize/gradient/#definitions-center
        https://typst.app/docs/reference/visualize/gradient/#definitions-conic
        https://typst.app/docs/reference/visualize/gradient/#definitions-focal-center
        https://typst.app/docs/reference/visualize/gradient/#definitions-focal-radius
        https://typst.app/docs/reference/visualize/gradient/#definitions-kind
        https://typst.app/docs/reference/visualize/gradient/#definitions-linear
        https://typst.app/docs/reference/visualize/gradient/#definitions-radial
        https://typst.app/docs/reference/visualize/gradient/#definitions-radius
        https://typst.app/docs/reference/visualize/gradient/#definitions-relative
        https://typst.app/docs/reference/visualize/gradient/#definitions-repeat
        https://typst.app/docs/reference/visualize/gradient/#definitions-sample
        https://typst.app/docs/reference/visualize/gradient/#definitions-samples
        https://typst.app/docs/reference/visualize/gradient/#definitions-sharp
        https://typst.app/docs/reference/visualize/gradient/#definitions-space
        https://typst.app/docs/reference/visualize/gradient/#definitions-stops
        https://typst.app/docs/reference/visualize/image/
        https://typst.app/docs/reference/visualize/image/#definitions-decode
        https://typst.app/docs/reference/visualize/line/
        https://typst.app/docs/reference/visualize/path/
        https://typst.app/docs/reference/visualize/pattern/
        https://typst.app/docs/reference/visualize/polygon/
        https://typst.app/docs/reference/visualize/polygon/#definitions-regular
        https://typst.app/docs/reference/visualize/rect/
        https://typst.app/docs/reference/visualize/square/
        https://typst.app/docs/reference/visualize/stroke/
        "###);
    }
}
