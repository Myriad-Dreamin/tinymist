use core::fmt;
use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    path::{Path, PathBuf},
};

use once_cell::sync::Lazy;
pub use serde::Serialize;
use serde_json::{ser::PrettyFormatter, Serializer, Value};
use typst::{
    diag::FileResult,
    syntax::{
        ast::{self, AstNode},
        FileId as TypstFileId, LinkedNode, Source, SyntaxKind, VirtualPath,
    },
};
use typst::{diag::PackageError, foundations::Bytes};
use typst_ts_compiler::{
    service::{CompileDriver, Compiler, EntryManager},
    NotifyApi, ShadowApi,
};
use typst_ts_core::{
    config::compiler::{EntryOpts, EntryState},
    package::Registry,
};
use typst_ts_core::{config::CompileOpts, package::PackageSpec};

pub use insta::assert_snapshot;
pub use typst_ts_compiler::TypstSystemWorld;

use crate::{
    analysis::{Analysis, AnalysisResources},
    prelude::AnalysisContext,
    typst_to_lsp, LspPosition, PositionEncoding,
};

struct WrapWorld<'a>(&'a mut TypstSystemWorld);

impl<'a> AnalysisResources for WrapWorld<'a> {
    fn world(&self) -> &dyn typst::World {
        self.0
    }

    fn resolve(&self, spec: &PackageSpec) -> Result<std::sync::Arc<Path>, PackageError> {
        self.0.registry.resolve(spec)
    }

    fn iter_dependencies<'b>(
        &'b self,
        f: &mut dyn FnMut(&'b reflexo::ImmutPath, FileResult<&typst_ts_compiler::Time>),
    ) {
        self.0.iter_dependencies(f)
    }
}

pub fn snapshot_testing(name: &str, f: &impl Fn(&mut AnalysisContext, PathBuf)) {
    let mut settings = insta::Settings::new();
    settings.set_prepend_module_to_snapshot(false);
    settings.set_snapshot_path(format!("fixtures/{name}/snaps"));
    settings.bind(|| {
        let glob_path = format!("fixtures/{name}/*.typ");
        insta::glob!(&glob_path, |path| {
            let contents = std::fs::read_to_string(path).unwrap();
            #[cfg(windows)]
            let contents = contents.replace("\r\n", "\n");

            run_with_sources(&contents, |w: &mut TypstSystemWorld, p| {
                let root = w.workspace_root().unwrap();
                let paths = w
                    .shadow_paths()
                    .into_iter()
                    .map(|p| {
                        TypstFileId::new(None, VirtualPath::new(p.strip_prefix(&root).unwrap()))
                    })
                    .collect::<Vec<_>>();
                let w = WrapWorld(w);
                let mut ctx = AnalysisContext::new(
                    &w,
                    Analysis {
                        root,
                        position_encoding: PositionEncoding::Utf16,
                        enable_periscope: false,
                        caches: Default::default(),
                    },
                );
                ctx.test_completion_files(Vec::new);
                ctx.test_files(|| paths);
                f(&mut ctx, p);
            });
        });
    });
}

pub fn get_test_properties(s: &str) -> HashMap<&'_ str, &'_ str> {
    let mut props = HashMap::new();
    for line in s.lines() {
        let mut line = line.splitn(2, ':');
        let key = line.next().unwrap().trim();
        let value = line.next().unwrap().trim();
        props.insert(key, value);
    }
    props
}

pub fn run_with_sources<T>(source: &str, f: impl FnOnce(&mut TypstSystemWorld, PathBuf) -> T) -> T {
    let root = if cfg!(windows) {
        PathBuf::from("C:\\")
    } else {
        PathBuf::from("/")
    };
    let mut world = TypstSystemWorld::new(CompileOpts {
        entry: EntryOpts::new_rooted(root.as_path().into(), None),
        ..Default::default()
    })
    .unwrap();
    let sources = source.split("-----");

    let pw = root.join(Path::new("/main.typ"));
    world.map_shadow(&pw, Bytes::from_static(b"")).unwrap();

    let mut last_pw = None;
    for (i, source) in sources.enumerate() {
        // find prelude
        let mut source = source.trim();
        let mut path = None;

        if source.starts_with("//") {
            let first_line = source.lines().next().unwrap();
            let content = first_line.strip_prefix("//").unwrap().trim();

            if let Some(path_attr) = content.strip_prefix("path:") {
                source = source.strip_prefix(first_line).unwrap().trim();
                path = Some(path_attr.trim().to_owned())
            }
        };

        let path = path.unwrap_or_else(|| format!("/s{i}.typ"));

        let pw = root.join(Path::new(&path));
        world
            .map_shadow(&pw, Bytes::from(source.as_bytes()))
            .unwrap();
        last_pw = Some(pw);
    }

    world.mutate_entry(EntryState::new_detached()).unwrap();
    let mut driver = CompileDriver::new(world);
    let _ = driver.compile(&mut Default::default());

    let pw = last_pw.unwrap();
    driver
        .world_mut()
        .mutate_entry(EntryState::new_rooted(
            root.as_path().into(),
            Some(TypstFileId::new(
                None,
                VirtualPath::new(pw.strip_prefix(root).unwrap()),
            )),
        ))
        .unwrap();
    f(driver.world_mut(), pw)
}

pub fn find_test_range(s: &Source) -> Range<usize> {
    // /* range -3..-1 */
    fn find_prefix(s: &str, sub: &str, left: bool) -> Option<(usize, usize, bool)> {
        Some((s.find(sub)?, sub.len(), left))
    }
    let (re_base, re_len, is_after) = find_prefix(s.text(), "/* range after ", true)
        .or_else(|| find_prefix(s.text(), "/* range ", false))
        .unwrap();
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
        .map(|e| (e, MatchAny { prev: true }));
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
    let mut n = n.leaf_at(re + 1).unwrap();

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
            n = n.children().last().unwrap();
            continue;
        }
        if match_ident {
            match n.kind() {
                SyntaxKind::Closure => {
                    let c = n.cast::<ast::Closure>().unwrap();
                    if let Some(name) = c.name() {
                        if let Some(m) = n.find(name.span()) {
                            n = m;
                            break 'match_loop;
                        }
                    }
                }
                SyntaxKind::LetBinding => {
                    let c = n.cast::<ast::LetBinding>().unwrap();
                    if let Some(name) = c.kind().bindings().first() {
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

    typst_to_lsp::offset_to_position(n.offset() + offset, PositionEncoding::Utf16, s)
}

// pub static REDACT_URI: Lazy<RedactFields> = Lazy::new(||
// RedactFields::from_iter(["uri"]));
pub static REDACT_LOC: Lazy<RedactFields> = Lazy::new(|| {
    RedactFields::from_iter([
        "location",
        "uri",
        "range",
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

    // pub fn new(v: impl serde::Serialize) -> Self {
    //     let s = serde_json::to_value(v).unwrap();
    //     Self(REDACT_URI.redact(s))
    // }

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
    fn redact(&self, v: Value) -> Value {
        match v {
            Value::Object(mut m) => {
                for (_, v) in m.iter_mut() {
                    *v = self.redact(v.clone());
                }
                for k in self.0.iter().copied() {
                    let Some(t) = m.remove(k) else {
                        continue;
                    };

                    match k {
                        "range"
                        | "selectionRange"
                        | "originSelectionRange"
                        | "targetRange"
                        | "targetSelectionRange" => {
                            m.insert(
                                k.to_owned(),
                                format!("{}:{}", pos(&t["start"]), pos(&t["end"])).into(),
                            );
                        }
                        _ => {}
                    }
                }
                Value::Object(m)
            }
            Value::Array(mut a) => {
                for v in a.iter_mut() {
                    *v = self.redact(v.clone());
                }
                Value::Array(a)
            }
            Value::String(s) => Value::String(s),
            v => v,
        }
    }
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
