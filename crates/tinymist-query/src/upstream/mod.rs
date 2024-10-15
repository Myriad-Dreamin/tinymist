use std::{collections::HashMap, fmt::Write};

use comemo::Tracked;
use ecow::{eco_format, EcoString};
use once_cell::sync::Lazy;
use serde::Deserialize;
use serde_yaml as yaml;
use typst::{
    diag::{bail, StrResult},
    foundations::{Content, Func, Module, Type, Value},
    introspection::MetadataElem,
    syntax::Span,
    text::{FontInfo, FontStyle},
    Library, World,
};

mod tooltip;
pub use tooltip::*;
mod complete;
pub use complete::*;

/// Extract the first sentence of plain text of a piece of documentation.
///
/// Removes Markdown formatting.
pub fn plain_docs_sentence(docs: &str) -> EcoString {
    log::debug!("plain docs {docs:?}");
    let docs = docs.replace("```example", "```typ");
    let mut s = unscanny::Scanner::new(&docs);
    let mut output = EcoString::new();
    let mut link = false;
    while let Some(c) = s.eat() {
        match c {
            '`' => {
                let mut raw = s.eat_until('`');
                if (raw.starts_with('{') && raw.ends_with('}'))
                    || (raw.starts_with('[') && raw.ends_with(']'))
                {
                    raw = &raw[1..raw.len() - 1];
                }

                s.eat();
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
                let c = s.cursor();
                if s.eat_if('(') {
                    s.eat_until(')');
                    let link_content = s.from(c + 1);
                    s.eat();

                    log::info!("Intra Link: {link_content}");
                    let link = resolve(link_content, "https://typst.app/docs/").ok();
                    let link = link.unwrap_or_else(|| {
                        log::warn!("Failed to resolve link: {link_content}");
                        "https://typst.app/docs/404.html".to_string()
                    });

                    output.push('(');
                    output.push_str(&link);
                    output.push(')');
                } else if s.eat_if('[') {
                    s.eat_until(']');
                    s.eat();
                    output.push_str(s.from(c));
                }
                link = false
            }
            // '*' | '_' => {}
            // '.' => {
            //     output.push('.');
            //     break;
            // }
            _ => output.push(c),
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

static GROUPS: Lazy<Vec<GroupData>> = Lazy::new(|| {
    let mut groups: Vec<GroupData> = yaml::from_str(include_str!("groups.yml")).unwrap();
    for group in &mut groups {
        if group.filter.is_empty() {
            group.filter = group
                .module()
                .scope()
                .iter()
                .filter(|(_, v)| matches!(v, Value::Func(_)))
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

static LIBRARY: Lazy<Library> = Lazy::new(Library::default);

/// Extract a module from another module.
#[track_caller]
fn get_module<'a>(parent: &'a Module, name: &str) -> StrResult<&'a Module> {
    match parent.scope().get(name) {
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
            category = focus.scope().get_category(name);
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
    let value = focus.field(name)?;

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
        if let Ok(field) = value.field(next) {
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
static ROUTE_MAPS: Lazy<HashMap<CatKey, String>> = Lazy::new(|| {
    // todo: this is a false positive for clippy on LazyHash
    #[allow(clippy::mutable_key_type)]
    let mut map = HashMap::new();
    let mut scope_to_finds = vec![
        (LIBRARY.global.scope(), None, None),
        (LIBRARY.math.scope(), None, None),
    ];
    while let Some((scope, parent_name, cat)) = scope_to_finds.pop() {
        for (name, value) in scope.iter() {
            let cat = cat.or_else(|| scope.get_category(name));
            let name = urlify(name);
            match value {
                Value::Func(f) => {
                    if let Some(cat) = cat {
                        let Some(name) = f.name() else {
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
                            map.insert(CatKey::Func(f.clone()), route);
                            continue;
                        }

                        log::debug!("func: {f:?} -> {cat:?}");

                        let route = if let Some(parent_name) = &parent_name {
                            format!("reference/{}/{parent_name}/#definitions-{name}", cat.name())
                        } else {
                            format!("reference/{}/{name}/", cat.name())
                        };

                        map.insert(CatKey::Func(f.clone()), route);
                    }
                    if let Some(s) = f.scope() {
                        scope_to_finds.push((s, Some(name), cat));
                    }
                }
                Value::Type(t) => {
                    if let Some(cat) = cat {
                        log::debug!("type: {t:?} -> {cat:?}");

                        let route = if let Some(parent_name) = &parent_name {
                            format!("reference/{}/{parent_name}/#definitions-{name}", cat.name())
                        } else {
                            format!("reference/{}/{name}/", cat.name())
                        };
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

/// Turn a title into an URL fragment.
pub(crate) fn urlify(title: &str) -> EcoString {
    title
        .chars()
        .map(|c| c.to_ascii_lowercase())
        .map(|c| match c {
            'a'..='z' | '0'..='9' => c,
            _ => '-',
        })
        .collect()
}

pub fn route_of_value(k: &Value) -> Option<&'static String> {
    // ROUTE_MAPS.get(&CatKey::Func(k.clone()))
    let key = match k {
        Value::Func(f) => CatKey::Func(f.clone()),
        Value::Type(t) => CatKey::Type(*t),
        _ => return None,
    };

    ROUTE_MAPS.get(&key)
}

/// Create a short description of a font family.
fn summarize_font_family<'a>(variants: impl Iterator<Item = &'a FontInfo>) -> EcoString {
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

pub fn truncated_repr(value: &Value) -> EcoString {
    const _10MB: usize = 100 * 1024 * 1024;
    truncated_repr_::<_10MB>(value)
}

pub fn truncated_doc_repr(value: &Value) -> EcoString {
    const _128B: usize = 128;
    truncated_repr_::<_128B>(value)
}

/// Run a function with a VM instance in the world
pub fn with_vm<T>(world: Tracked<dyn World + '_>, f: impl FnOnce(&mut typst::eval::Vm) -> T) -> T {
    use comemo::Track;
    use typst::engine::*;
    use typst::eval::*;
    use typst::foundations::*;
    use typst::introspection::*;

    let mut locator = Locator::default();
    let introspector = Introspector::default();
    let mut tracer = Tracer::new();
    let engine = Engine {
        world,
        route: Route::default(),
        introspector: introspector.track(),
        locator: &mut locator,
        tracer: tracer.track_mut(),
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
}
