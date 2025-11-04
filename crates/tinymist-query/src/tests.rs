use core::fmt;
use std::borrow::Cow;
use std::str::FromStr;
use std::sync::{Arc, LazyLock};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    path::{Path, PathBuf},
};

use regex::{Regex, Replacer};
use serde_json::{Serializer, Value, ser::PrettyFormatter};
use tinymist_project::{LspCompileSnapshot, LspComputeGraph, LspWorld};
use tinymist_std::path::unix_slash;
use tinymist_std::typst::TypstDocument;
use tinymist_world::debug_loc::LspRange;
use tinymist_world::package::PackageSpec;
use tinymist_world::vfs::WorkspaceResolver;
use tinymist_world::{EntryReader, ShadowApi, TaskInputs};
use typst::syntax::ast::{self, AstNode};
use typst::syntax::{LinkedNode, Source, SyntaxKind, VirtualPath};
use typst_shim::syntax::LinkedNodeExt;

pub use serde::Serialize;
pub use serde_json::json;
pub use tinymist_project::LspUniverse;
pub use tinymist_tests::{assert_snapshot, run_with_sources, with_settings};
pub use tinymist_world::WorldComputeGraph;

pub use crate::syntax::find_module_level_docs;
use crate::{CompletionFeat, to_lsp_position, to_typst_position};
use crate::{LspPosition, PositionEncoding, analysis::Analysis, prelude::LocalContext};

#[derive(Default, Clone, Copy)]
pub struct Opts {
    pub need_compile: bool,
}

pub fn snapshot_testing(name: &str, f: &impl Fn(&mut LocalContext, PathBuf)) {
    tinymist_tests::snapshot_testing!(name, |verse, path| {
        run_with_ctx(verse, path, f);
    });
}

pub fn snapshot_testing_with(name: &str, opts: Opts, f: &impl Fn(&mut LocalContext, PathBuf)) {
    tinymist_tests::snapshot_testing!(name, |verse, path| {
        run_with_ctx_(verse, path, opts, f);
    });
}

pub fn run_with_ctx<T>(
    verse: &mut LspUniverse,
    path: PathBuf,
    f: &impl Fn(&mut LocalContext, PathBuf) -> T,
) -> T {
    run_with_ctx_(verse, path, Opts::default(), f)
}

pub fn run_with_ctx_<T>(
    verse: &mut LspUniverse,
    path: PathBuf,
    opts: Opts,
    f: &impl Fn(&mut LocalContext, PathBuf) -> T,
) -> T {
    let root = verse.entry_state().workspace_root().unwrap();
    let paths = verse
        .shadow_paths()
        .into_iter()
        .map(|path| {
            WorkspaceResolver::workspace_file(
                Some(&root),
                VirtualPath::new(path.strip_prefix(&root).unwrap()),
            )
        })
        .collect::<Vec<_>>();

    let mut world = verse.snapshot();
    world.set_is_compiling(false);

    let source = world.source_by_path(&path).ok().unwrap();
    let docs = find_module_level_docs(&source).unwrap_or_default();
    let properties = get_test_properties(&docs);
    let supports_html = properties
        .get("html")
        .map(|v| v.trim() == "true")
        .unwrap_or(true);

    let g = compile_doc_for_test(&world, &properties, opts.need_compile);
    let a = Arc::new(Analysis {
        remove_html: !supports_html,
        completion_feat: CompletionFeat {
            trigger_on_snippet_placeholders: true,
            trigger_suggest: true,
            trigger_parameter_hints: true,
            trigger_suggest_and_parameter_hints: true,
            ..Default::default()
        },
        ..Analysis::default()
    });
    let mut ctx = a.enter_(g, a.lock_revision(None));

    ctx.test_package_list(|| {
        vec![(
            PackageSpec::from_str("@preview/example:0.1.0").unwrap(),
            Some("example package (mock).".into()),
        )]
    });
    ctx.test_completion_files(|| paths.clone());
    ctx.test_files(|| paths);
    f(&mut ctx, path)
}

pub fn get_test_properties(s: &str) -> HashMap<&'_ str, &'_ str> {
    let mut props = HashMap::new();
    for line in s.lines() {
        let mut line = line.splitn(2, ':');
        let key = line.next().unwrap().trim();
        let Some(value) = line.next() else {
            continue;
        };
        props.insert(key, value.trim());
    }
    props
}

pub fn compile_doc_for_test(
    world: &LspWorld,
    properties: &HashMap<&str, &str>,
    need_compile: bool,
) -> LspComputeGraph {
    let prev = world.entry_state();
    let next = match properties.get("compile").map(|s| s.trim()) {
        _ if need_compile => prev.clone(),
        Some("true") => prev.clone(),
        None | Some("false") => return WorldComputeGraph::from_world(world.clone()),
        Some(path) if path.ends_with(".typ") => prev.select_in_workspace(Path::new(path)),
        v => panic!("invalid value for 'compile' property: {v:?}"),
    };

    let mut world = Cow::Borrowed(world);
    if next != prev {
        world = Cow::Owned(world.task(TaskInputs {
            entry: Some(next),
            ..Default::default()
        }));
    }
    let mut snap = LspCompileSnapshot::from_world(world.into_owned());
    snap.world.set_is_compiling(true);

    let doc = typst::compile(&snap.world).output.unwrap();
    snap.success_doc = Some(TypstDocument::Paged(Arc::new(doc)));
    WorldComputeGraph::new(snap)
}

pub fn find_test_range(s: &Source) -> LspRange {
    let range = find_test_range_(s);
    crate::to_lsp_range(range, s, PositionEncoding::Utf16)
}

pub fn find_test_range_(s: &Source) -> Range<usize> {
    // /* range -3..-1 */
    fn find_prefix(s: &str, sub: &str, left: bool) -> Option<(usize, usize, bool)> {
        Some((s.find(sub)?, sub.len(), left))
    }
    let (re_base, re_len, is_after) = find_prefix(s.text(), "/* range after ", true)
        .or_else(|| find_prefix(s.text(), "/* range ", false))
        .unwrap_or_else(|| panic!("no range marker found in source:\n{}", s.text()));
    let re_end = re_base + re_len;
    let range_rng = re_end..(s.text()[re_end..].find(" */").unwrap() + re_end);
    let range_base = if is_after {
        range_rng.end + " */".len()
    } else {
        re_base
    };
    let range = &s.text()[range_rng];
    // split by ".."
    let mut bounds = range.split("..");
    // parse the range
    let start: isize = bounds.next().unwrap().parse().unwrap();
    let end: isize = bounds.next().unwrap().parse().unwrap();

    let start = start + range_base as isize;
    let end = end + range_base as isize;
    start as usize..end as usize
}

pub fn find_test_position_after(s: &Source) -> LspPosition {
    find_test_lsp_pos(s, 1)
}

pub fn find_test_position(s: &Source) -> LspPosition {
    find_test_lsp_pos(s, 0)
}

pub fn find_test_lsp_pos(s: &Source, offset: usize) -> LspPosition {
    let node = find_test_typst_pos(s);
    to_lsp_position(node + offset, PositionEncoding::Utf16, s)
}

pub fn find_test_typst_pos(s: &Source) -> usize {
    enum PosMatcher {
        Pos { prev: bool, ident: bool },
        LoC { line: i32, column: i32 },
    }
    use PosMatcher::*;

    fn pos(prev: bool, ident: bool) -> Option<PosMatcher> {
        Some(Pos { prev, ident })
    }

    let re = s.text().find("/* position */").zip(pos(true, false));
    let re = re.or_else(|| s.text().find("/* position after */").zip(pos(false, false)));
    let re = re.or_else(|| s.text().find("/* ident */").zip(pos(true, true)));
    let re = re.or_else(|| s.text().find("/* ident after */").zip(pos(false, true)));
    let re = re.or_else(|| {
        let re = s.text().find("/* loc ")?;
        let (parts, _) = s.text()[re + "/* loc ".len()..]
            .trim()
            .split_once("*/")
            .expect("bad loc marker");
        let (line, column) = parts.split_once(',').expect("bad loc marker");
        let line = line.trim().parse::<i32>().expect("bad loc marker");
        let column = column.trim().parse::<i32>().expect("bad loc marker");
        Some((re, LoC { line, column }))
    });

    let Some((rel_offset, matcher)) = re else {
        panic!("No (or bad) position marker found in source:\n{}", s.text())
    };

    match matcher {
        Pos { prev, ident } => {
            let node = LinkedNode::new(s.root());
            let node = node.leaf_at_compat(rel_offset + 1).unwrap();

            match_by_pos(node, prev, ident)
        }
        LoC { line, column } => {
            let column = if line != 0 { column } else { 0 };

            let rel_pos = to_lsp_position(rel_offset, PositionEncoding::Utf16, s);
            let pos = LspPosition {
                line: (rel_pos.line as i32 + line) as u32,
                character: (rel_pos.character as i32 + column) as u32,
            };
            to_typst_position(pos, PositionEncoding::Utf16, s).expect("invalid loc")
        }
    }
}

fn match_by_pos(mut n: LinkedNode, prev: bool, ident: bool) -> usize {
    'match_loop: loop {
        if n.kind().is_trivia() || n.kind().is_error() {
            let m = if prev {
                n.prev_sibling()
            } else {
                n.next_sibling()
            };
            n = m.or_else(|| n.parent().cloned()).unwrap();
            continue;
        }
        if matches!(n.kind(), SyntaxKind::Named) {
            if ident {
                n = n
                    .children()
                    .find(|n| matches!(n.kind(), SyntaxKind::Ident))
                    .unwrap();
            } else {
                n = n.children().next_back().unwrap();
            }
            continue;
        }
        if ident {
            match n.kind() {
                SyntaxKind::Closure => {
                    let closure = n.cast::<ast::Closure>().unwrap();
                    if let Some(name) = closure.name()
                        && let Some(m) = n.find(name.span())
                    {
                        n = m;
                        break 'match_loop;
                    }
                }
                SyntaxKind::LetBinding => {
                    let let_binding = n.cast::<ast::LetBinding>().unwrap();
                    if let Some(name) = let_binding.kind().bindings().first()
                        && let Some(m) = n.find(name.span())
                    {
                        n = m;
                        break 'match_loop;
                    }
                }
                _ => {}
            }
        }
        break;
    }

    n.offset()
}

pub fn make_pos_annotation(source: &Source) -> (LspPosition, String) {
    let pos = find_test_typst_pos(source);
    let range_before = pos.saturating_sub(10)..pos;
    let range_after = pos..pos.saturating_add(10).min(source.text().len());

    let window_before = &source.text()[range_before];
    let window_after = &source.text()[range_after];

    let pos = to_lsp_position(pos, PositionEncoding::Utf16, source);
    (pos, format!("{window_before}|{window_after}"))
}

pub fn make_range_annotation(source: &Source) -> String {
    let range = find_test_range_(source);
    let range_before = range.start.saturating_sub(10)..range.start;
    let range_window = range.clone();
    let range_after = range.end..range.end.saturating_add(10).min(source.text().len());

    let window_before = &source.text()[range_before];
    let window_line = &source.text()[range_window];
    let window_after = &source.text()[range_after];
    format!("{window_before}|{window_line}|{window_after}")
}

// pub static REDACT_URI: Lazy<RedactFields> = Lazy::new(||
// RedactFields::from_iter(["uri"]));
pub static REDACT_LOC: LazyLock<RedactFields> = LazyLock::new(|| {
    RedactFields::from_iter([
        "location",
        "contents",
        "file",
        "uri",
        "oldUri",
        "newUri",
        "range",
        "changes",
        "selectionRange",
        "targetRange",
        "targetSelectionRange",
        "originSelectionRange",
        "target",
        "targetUri",
    ])
});

pub struct JsonRepr(Value);

impl JsonRepr {
    pub fn new_pure(v: impl serde::Serialize) -> Self {
        let s = serde_json::to_value(v).unwrap();
        Self(s)
    }

    pub fn new_redacted(v: impl serde::Serialize, rm: &RedactFields) -> Self {
        let s = serde_json::to_value(v).unwrap();
        Self(rm.redact(s))
    }

    pub fn md_content(v: &str) -> Cow<'_, str> {
        static REG: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r#"data:image/svg\+xml;base64,([^"]+)"#).unwrap());
        static REG2: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r#"C:\\?\\dummy-root\\?\\"#).unwrap());
        let v = REG.replace_all(
            v,
            |_captures: &regex::Captures| "data:image-hash/svg+xml;base64,redacted",
        );
        REG2.replace_all_cow(v, "/dummy-root/")
    }

    pub fn range(v: impl serde::Serialize) -> String {
        let t = serde_json::to_value(v).unwrap();
        Self::range_(&t)
    }

    pub fn range_(t: &Value) -> String {
        format!("{}:{}", pos(&t["start"]), pos(&t["end"]))
    }
}

impl fmt::Display for JsonRepr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let w = std::io::BufWriter::new(Vec::new());
        let mut ser = Serializer::with_formatter(w, PrettyFormatter::with_indent(b" "));
        self.0.serialize(&mut ser).unwrap();

        let res = String::from_utf8(ser.into_inner().into_inner().unwrap()).unwrap();
        // replace Span(number) to Span(..)
        static REG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r#"Span\((\d+)\)"#).unwrap());
        let res = REG.replace_all(&res, "Span(..)");
        f.write_str(&res)
    }
}

pub trait Redact {
    fn redact(&self, v: Value) -> Value;
}

pub struct RedactFields(HashSet<&'static str>);

impl FromIterator<&'static str> for RedactFields {
    fn from_iter<T: IntoIterator<Item = &'static str>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

fn pos(v: &Value) -> String {
    match v {
        Value::Object(v) => format!("{}:{}", v["line"], v["character"]),
        Value::Number(v) => v.to_string(),
        _ => "<null>".to_owned(),
    }
}

impl Redact for RedactFields {
    fn redact(&self, json_val: Value) -> Value {
        match json_val {
            Value::Object(mut map) => {
                for (_, val) in map.iter_mut() {
                    *val = self.redact(val.clone());
                }

                if let Some(kind) = map.get("kind")
                    && matches!(kind.as_str(), Some("pathAt"))
                {
                    if let Some(value) = map.get("value")
                        && let Value::String(s) = value
                    {
                        let v = file_path_(Path::new(s)).into();
                        map.insert("value".to_owned(), v);
                    }

                    if let Some(error) = map.get("error") {
                        let error = error.as_str().unwrap();
                        static REG: LazyLock<Regex> = LazyLock::new(|| {
                            Regex::new(r#"(/dummy-root/|C:\\dummy-root\\).*?\.typ"#).unwrap()
                        });
                        let error = REG.replace_all(error, "/__redacted_path__typ");
                        map.insert("error".to_owned(), Value::String(error.into()));
                    }
                }

                for key in self.0.iter().copied() {
                    let Some(t) = map.remove(key) else {
                        continue;
                    };

                    match key {
                        "changes" => {
                            let obj = t.as_object().unwrap();
                            map.insert(
                                key.to_owned(),
                                Value::Object(
                                    obj.iter().map(|(k, v)| (file_uri(k), v.clone())).collect(),
                                ),
                            );
                        }
                        "file" => {
                            map.insert(
                                key.to_owned(),
                                file_path_(Path::new(t.as_str().unwrap())).into(),
                            );
                        }
                        "uri" | "target" | "oldUri" | "newUri" | "targetUri" => {
                            map.insert(key.to_owned(), file_uri(t.as_str().unwrap()).into());
                        }
                        "range"
                        | "selectionRange"
                        | "originSelectionRange"
                        | "targetRange"
                        | "targetSelectionRange" => {
                            map.insert(key.to_owned(), JsonRepr::range_(&t).into());
                        }
                        "contents" => {
                            let res = t.as_str().unwrap();
                            map.insert(key.to_owned(), JsonRepr::md_content(res).into());
                        }
                        _ => {}
                    }
                }
                Value::Object(map)
            }
            Value::Array(mut arr) => {
                for elem in arr.iter_mut() {
                    *elem = self.redact(elem.clone());
                }
                Value::Array(arr)
            }
            Value::String(content) => Value::String(content),
            json_val => json_val,
        }
    }
}

pub(crate) fn file_uri(uri: &str) -> String {
    file_uri_(&lsp_types::Url::parse(uri).unwrap())
}

pub(crate) fn file_uri_(uri: &lsp_types::Url) -> String {
    let uri = uri.to_file_path().unwrap();
    file_path_(&uri)
}

pub(crate) fn file_path_(path: &Path) -> String {
    let root = if cfg!(windows) {
        PathBuf::from("C:\\dummy-root")
    } else {
        PathBuf::from("/dummy-root")
    };
    let abs_path = path.strip_prefix(root).map(|p| p.to_owned());
    let rel_path =
        abs_path.unwrap_or_else(|_| Path::new("-").join(path.iter().next_back().unwrap()));

    unix_slash(&rel_path)
}

/// Extension methods for `Regex` that operate on `Cow<str>` instead of `&str`.
pub trait RegexCowExt {
    /// [`Regex::replace_all`], but taking text as `Cow<str>` instead of `&str`.
    fn replace_all_cow<'t, R: Replacer>(&self, text: Cow<'t, str>, rep: R) -> Cow<'t, str>;
}

impl RegexCowExt for Regex {
    fn replace_all_cow<'t, R: Replacer>(&self, text: Cow<'t, str>, rep: R) -> Cow<'t, str> {
        match self.replace_all(&text, rep) {
            Cow::Owned(result) => Cow::Owned(result),
            Cow::Borrowed(_) => text,
        }
    }
}
