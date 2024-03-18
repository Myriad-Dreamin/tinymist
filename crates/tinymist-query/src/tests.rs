use core::fmt;
use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use once_cell::sync::Lazy;
use serde::Serialize;
use serde_json::{ser::PrettyFormatter, Serializer, Value};
use typst::syntax::{
    ast::{self, AstNode},
    LinkedNode, Source, SyntaxKind, VirtualPath,
};
use typst_ts_compiler::{
    service::{CompileDriver, Compiler, WorkspaceProvider},
    ShadowApi,
};
use typst_ts_core::{config::CompileOpts, Bytes, TypstFileId};

pub use insta::assert_snapshot;
pub use typst_ts_compiler::TypstSystemWorld;

use crate::{
    analysis::Analysis, prelude::AnalysisContext, typst_to_lsp, LspPosition, PositionEncoding,
};

pub fn snapshot_testing(name: &str, f: &impl Fn(&mut AnalysisContext, PathBuf)) {
    let mut settings = insta::Settings::new();
    settings.set_prepend_module_to_snapshot(false);
    settings.set_snapshot_path(format!("fixtures/{name}/snaps"));
    settings.bind(|| {
        let glob_path = format!("fixtures/{name}/*.typ");
        insta::glob!(&glob_path, |path| {
            let contents = std::fs::read_to_string(path).unwrap();

            run_with_sources(&contents, |w: &mut TypstSystemWorld, p| {
                let paths = w
                    .shadow_paths()
                    .into_iter()
                    .map(|p| {
                        TypstFileId::new(
                            None,
                            VirtualPath::new(p.strip_prefix(w.workspace_root()).unwrap()),
                        )
                    })
                    .collect::<Vec<_>>();
                let mut ctx = AnalysisContext::new(
                    w,
                    Analysis {
                        root: w.workspace_root(),
                        position_encoding: PositionEncoding::Utf16,
                    },
                );
                ctx.test_files(|| paths);
                f(&mut ctx, p);
            });
        });
    });
}

pub fn run_with_sources<T>(source: &str, f: impl FnOnce(&mut TypstSystemWorld, PathBuf) -> T) -> T {
    let root = if cfg!(windows) {
        PathBuf::from("C:\\")
    } else {
        PathBuf::from("/")
    };
    let mut world = TypstSystemWorld::new(CompileOpts {
        root_dir: root.clone(),
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
            source = source.strip_prefix(first_line).unwrap().trim();

            let content = first_line.strip_prefix("//").unwrap().trim();
            path = content.strip_prefix("path:").map(|e| e.trim().to_owned())
        };

        let path = path.unwrap_or_else(|| format!("/s{i}.typ"));

        let pw = root.join(Path::new(&path));
        world
            .map_shadow(&pw, Bytes::from(source.as_bytes()))
            .unwrap();
        last_pw = Some(pw);
    }

    world.set_main_id(TypstFileId::new(None, VirtualPath::new("/main.typ")));
    let mut driver = CompileDriver::new(world);
    let _ = driver.compile(&mut Default::default());

    let pw = last_pw.unwrap();
    driver.world_mut().set_main_id(TypstFileId::new(
        None,
        VirtualPath::new(pw.strip_prefix(root).unwrap()),
    ));
    f(driver.world_mut(), pw)
}

pub fn find_test_position(s: &Source) -> LspPosition {
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
        if n.kind().is_trivia() {
            let m = if match_prev {
                n.prev_sibling()
            } else {
                n.next_sibling()
            };
            n = m.or_else(|| n.parent().cloned()).unwrap();
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

    // eprintln!("position: {:?} -> {:?}", n.offset(), n);

    typst_to_lsp::offset_to_position(n.offset(), PositionEncoding::Utf16, s)
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
