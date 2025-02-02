use core::fmt;
use std::borrow::Cow;
use std::str::FromStr;
use std::sync::{Arc, LazyLock};
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    path::{Path, PathBuf},
};

use once_cell::sync::Lazy;
use serde_json::{ser::PrettyFormatter, Serializer, Value};
use tinymist_project::CompileFontArgs;
use tinymist_std::typst::TypstDocument;
use tinymist_world::package::PackageSpec;
use tinymist_world::vfs::WorkspaceResolver;
use tinymist_world::EntryState;
use tinymist_world::TaskInputs;
use tinymist_world::{EntryManager, EntryReader, ShadowApi};
use typst::foundations::Bytes;
use typst::syntax::ast::{self, AstNode};
use typst::syntax::{LinkedNode, Source, SyntaxKind, VirtualPath};

pub use insta::assert_snapshot;
pub use serde::Serialize;
pub use serde_json::json;
pub use tinymist_project::{LspUniverse, LspUniverseBuilder};
use typst_shim::syntax::LinkedNodeExt;

use crate::syntax::find_module_level_docs;
use crate::{
    analysis::Analysis, prelude::LocalContext, LspPosition, PositionEncoding, VersionedDocument,
};
use crate::{to_lsp_position, CompletionFeat, LspWorldExt};

pub fn snapshot_testing(name: &str, f: &impl Fn(&mut LocalContext, PathBuf)) {
    let name = if name.is_empty() { "playground" } else { name };

    let mut settings = insta::Settings::new();
    settings.set_prepend_module_to_snapshot(false);
    settings.set_snapshot_path(format!("fixtures/{name}/snaps"));
    settings.bind(|| {
        let glob_path = format!("fixtures/{name}/*.typ");
        insta::glob!(&glob_path, |path| {
            let contents = std::fs::read_to_string(path).unwrap();
            #[cfg(windows)]
            let contents = contents.replace("\r\n", "\n");

            run_with_sources(&contents, |verse, path| {
                run_with_ctx(verse, path, f);
            });
        });
    });
}

pub fn run_with_ctx<T>(
    verse: &mut LspUniverse,
    path: PathBuf,
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

    let mut ctx = Arc::new(Analysis {
        remove_html: !supports_html,
        completion_feat: CompletionFeat {
            trigger_on_snippet_placeholders: true,
            trigger_suggest: true,
            trigger_parameter_hints: true,
            trigger_suggest_and_parameter_hints: true,
            ..Default::default()
        },
        ..Analysis::default()
    })
    .snapshot(world);

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
    ctx: &mut LocalContext,
    properties: &HashMap<&str, &str>,
) -> Option<VersionedDocument> {
    let prev = ctx.world.entry_state();
    let next = match properties.get("compile")?.trim() {
        "true" => prev.clone(),
        "false" => return None,
        path if path.ends_with(".typ") => prev.select_in_workspace(Path::new(path)),
        v => panic!("invalid value for 'compile' property: {v}"),
    };

    let mut world = Cow::Borrowed(&ctx.world);
    if next != prev {
        world = Cow::Owned(world.task(TaskInputs {
            entry: Some(next),
            ..Default::default()
        }));
    }
    let mut world = world.into_owned();
    world.set_is_compiling(true);

    let doc = typst::compile(&world).output.unwrap();
    Some(VersionedDocument {
        version: 0,
        document: Arc::new(TypstDocument::Paged(Arc::new(doc))),
    })
}

pub fn run_with_sources<T>(source: &str, f: impl FnOnce(&mut LspUniverse, PathBuf) -> T) -> T {
    let root = if cfg!(windows) {
        PathBuf::from("C:\\")
    } else {
        PathBuf::from("/")
    };
    let mut verse = LspUniverseBuilder::build(
        EntryState::new_rooted(root.as_path().into(), None),
        Default::default(),
        Arc::new(
            LspUniverseBuilder::resolve_fonts(CompileFontArgs {
                ignore_system_fonts: true,
                ..Default::default()
            })
            .unwrap(),
        ),
        LspUniverseBuilder::resolve_package(None, None),
    );
    let sources = source.split("-----");

    let mut last_pw = None;
    for (idx, source) in sources.enumerate() {
        // find prelude
        let mut source = source.trim_start();
        let mut path = None;

        if source.starts_with("//") {
            let first_line = source.lines().next().unwrap();
            let content = first_line.trim_start_matches("/").trim();

            if let Some(path_attr) = content.strip_prefix("path:") {
                source = source.strip_prefix(first_line).unwrap().trim();
                path = Some(path_attr.trim().to_owned())
            }
        };

        let path = path.unwrap_or_else(|| format!("/s{idx}.typ"));

        let pw = root.join(Path::new(&path));
        verse
            .map_shadow(&pw, Bytes::from_string(source.to_owned()))
            .unwrap();
        last_pw = Some(pw);
    }

    let pw = last_pw.unwrap();
    verse
        .mutate_entry(EntryState::new_rooted(
            root.as_path().into(),
            Some(VirtualPath::new(pw.strip_prefix(root).unwrap())),
        ))
        .unwrap();
    f(&mut verse, pw)
}

pub fn find_test_range(s: &Source) -> Range<usize> {
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
    find_test_position_(s, 1)
}

pub fn find_test_position(s: &Source) -> LspPosition {
    find_test_position_(s, 0)
}

pub fn find_test_position_(s: &Source, offset: usize) -> LspPosition {
    enum AstMatcher {
        MatchAny { prev: bool },
        MatchIdent { prev: bool },
    }
    use AstMatcher::*;

    let re = s
        .text()
        .find("/* position */")
        .zip(Some(MatchAny { prev: true }));
    let re = re.or_else(|| {
        s.text()
            .find("/* position after */")
            .zip(Some(MatchAny { prev: false }))
    });
    let re = re.or_else(|| {
        s.text()
            .find("/* ident */")
            .zip(Some(MatchIdent { prev: true }))
    });
    let re = re.or_else(|| {
        s.text()
            .find("/* ident after */")
            .zip(Some(MatchIdent { prev: false }))
    });
    let (re, m) = re
        .ok_or_else(|| panic!("No position marker found in source:\n{}", s.text()))
        .unwrap();

    let n = LinkedNode::new(s.root());
    let mut n = n.leaf_at_compat(re + 1).unwrap();

    let match_prev = match &m {
        MatchAny { prev } => *prev,
        MatchIdent { prev } => *prev,
    };
    let match_ident = match m {
        MatchAny { .. } => false,
        MatchIdent { .. } => true,
    };

    'match_loop: loop {
        if n.kind().is_trivia() || n.kind().is_error() {
            let m = if match_prev {
                n.prev_sibling()
            } else {
                n.next_sibling()
            };
            n = m.or_else(|| n.parent().cloned()).unwrap();
            continue;
        }
        if matches!(n.kind(), SyntaxKind::Named) {
            if match_ident {
                n = n
                    .children()
                    .find(|n| matches!(n.kind(), SyntaxKind::Ident))
                    .unwrap();
            } else {
                n = n.children().last().unwrap();
            }
            continue;
        }
        if match_ident {
            match n.kind() {
                SyntaxKind::Closure => {
                    let closure = n.cast::<ast::Closure>().unwrap();
                    if let Some(name) = closure.name() {
                        if let Some(m) = n.find(name.span()) {
                            n = m;
                            break 'match_loop;
                        }
                    }
                }
                SyntaxKind::LetBinding => {
                    let let_binding = n.cast::<ast::LetBinding>().unwrap();
                    if let Some(name) = let_binding.kind().bindings().first() {
                        if let Some(m) = n.find(name.span()) {
                            n = m;
                            break 'match_loop;
                        }
                    }
                }
                _ => {}
            }
        }
        break;
    }

    to_lsp_position(n.offset() + offset, PositionEncoding::Utf16, s)
}

// pub static REDACT_URI: Lazy<RedactFields> = Lazy::new(||
// RedactFields::from_iter(["uri"]));
pub static REDACT_LOC: Lazy<RedactFields> = Lazy::new(|| {
    RedactFields::from_iter([
        "location",
        "contents",
        "uri",
        "oldUri",
        "newUri",
        "range",
        "changes",
        "selectionRange",
        "targetRange",
        "targetSelectionRange",
        "originSelectionRange",
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
}

impl fmt::Display for JsonRepr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let w = std::io::BufWriter::new(Vec::new());
        let mut ser = Serializer::with_formatter(w, PrettyFormatter::with_indent(b" "));
        self.0.serialize(&mut ser).unwrap();

        f.write_str(&String::from_utf8(ser.into_inner().into_inner().unwrap()).unwrap())
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
        static REG: LazyLock<regex::Regex> =
            LazyLock::new(|| regex::Regex::new(r#"data:image/svg\+xml;base64,([^"]+)"#).unwrap());
        match json_val {
            Value::Object(mut map) => {
                for (_, val) in map.iter_mut() {
                    *val = self.redact(val.clone());
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
                                    obj.iter().map(|(k, v)| (file_name(k), v.clone())).collect(),
                                ),
                            );
                        }
                        "uri" | "oldUri" | "newUri" | "targetUri" => {
                            map.insert(key.to_owned(), file_name(t.as_str().unwrap()).into());
                        }
                        "range"
                        | "selectionRange"
                        | "originSelectionRange"
                        | "targetRange"
                        | "targetSelectionRange" => {
                            map.insert(
                                key.to_owned(),
                                format!("{}:{}", pos(&t["start"]), pos(&t["end"])).into(),
                            );
                        }
                        "contents" => {
                            let res = t.as_str().unwrap();
                            let res = REG.replace_all(res, |_captures: &regex::Captures| {
                                "data:image-hash/svg+xml;base64,redacted"
                            });

                            map.insert(key.to_owned(), res.into());
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

fn file_name(path: &str) -> String {
    let name = Path::new(path).file_name().unwrap();
    name.to_str().unwrap().to_owned()
}

pub struct HashRepr<T>(pub T);

// sha256
impl fmt::Display for HashRepr<JsonRepr> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use sha2::{Digest, Sha256};

        let res = self.0.to_string();
        let hash = Sha256::digest(res).to_vec();
        write!(f, "sha256:{}", hex::encode(hash))
    }
}
