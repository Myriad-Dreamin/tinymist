//! Runs hook scripts for the server.

mod export;

use std::ops::Deref;

pub use export::*;
use tinymist_world::diag::print_diagnostics_to_string;
use tinymist_world::vfs::WorkspaceResolver;

use crate::prelude::*;
use comemo::Track;
use tinymist_project::LspWorld;
use tinymist_std::error::prelude::*;
use tinymist_world::{DiagnosticFormat, EntryState, ShadowApi};
use typst::World;
use typst::diag::{At, SourceResult};
use typst::foundations::{Args, Bytes, Context, Dict, Func, NativeFunc, eco_format};
use typst::syntax::Span;
use typst::syntax::SyntaxKind;
use typst::syntax::SyntaxNode;
use typst::syntax::ast;
use typst::utils::LazyHash;
use typst_shim::eval::{Eval, Vm};

/// The hook script.
pub enum HookScript<'a> {
    /// A code script.
    Code(&'a str),
    /// A function callback.
    Callback(Func),
}

/// Evaluates a hook script.
pub fn eval_script(
    world: &LspWorld,
    code: HookScript,
    inputs: Dict,
    entry: &EntryState,
) -> Result<(LspWorld, Value)> {
    let id = entry
        .select_in_workspace(Path::new("/__script__.typ"))
        .main()
        .expect("cannot create script file id");

    let inputs = make_sys(entry, world.inputs(), inputs);
    let (inputs, root, dir, name) = match inputs {
        Some(EvalSysCtx {
            inputs,
            root,
            dir,
            name,
        }) => (Some(inputs), Some(root), dir, Some(name)),
        None => (None, None, None, None),
    };

    let mut world = world.task(tinymist_world::TaskInputs {
        entry: None,
        inputs,
    });
    if let HookScript::Code(code) = &code {
        // todo: bad performance
        world.take_db();
        let _ = world.map_shadow_by_id(id, Bytes::from_string((*code).to_owned()));
    }

    let res = tinymist_analysis::upstream::with_vm((&world as &dyn World).track(), |vm| {
        define_val(vm, "join", Value::Func(join::data().into()));
        define_val(vm, "debounce", Value::Func(debounce::data().into()));
        for (key, value) in [("root", root), ("dir", dir), ("name", name)] {
            if let Some(value) = value {
                define_val(vm, key, value);
            }
        }

        let res = match code {
            HookScript::Code(code) => {
                let mut expr = typst::syntax::parse_code(code);
                let span = Span::from_range(id, 0..code.len());
                expr.synthesize(span);

                let expr = match expr.cast::<ast::Code>() {
                    Some(v) => v,
                    None => bail!(
                        "code is not a valid code expression: kind={:?}",
                        expr.kind()
                    ),
                };
                expr.eval(vm)
            }
            HookScript::Callback(callback) => callback.call(
                &mut vm.engine,
                Context::default().track(),
                Vec::<Value>::default(),
            ),
        };
        match res {
            Ok(value) => Ok(value),
            Err(e) => {
                let res = print_diagnostics_to_string(&world, e.iter(), DiagnosticFormat::Human);
                let err = res.unwrap_or_else(|e| e);
                bail!("failed to evaluate expression: {err}")
            }
        }
    })?;
    Ok((world, res))
}

#[derive(Debug, Clone, Hash)]
struct EvalSysCtx {
    inputs: Arc<LazyHash<Dict>>,
    root: Value,
    dir: Option<Value>,
    name: Value,
}

#[comemo::memoize]
fn make_sys(entry: &EntryState, base: Arc<LazyHash<Dict>>, inputs: Dict) -> Option<EvalSysCtx> {
    let root = entry.root();
    let main = entry.main();

    log::debug!("Check path {main:?} and root {root:?}");

    let (root, main) = root.zip(main)?;

    // Files in packages are not exported
    if WorkspaceResolver::is_package_file(main) {
        return None;
    }
    // Files without a path are not exported
    let path = main.vpath().resolve(&root)?;

    // todo: handle untitled path
    if path.strip_prefix("/untitled").is_ok() {
        return None;
    }

    let path = path.strip_prefix(&root).ok()?;
    let dir = path.parent();
    let file_name = path.file_name().unwrap_or_default();

    let root = Value::Str(root.to_string_lossy().into());

    let dir = dir.map(|d| Value::Str(d.to_string_lossy().into()));

    let name = file_name.to_string_lossy();
    let name = name.as_ref().strip_suffix(".typ").unwrap_or(name.as_ref());
    let name = Value::Str(name.into());

    let mut dict = base.as_ref().deref().clone();
    for (key, value) in inputs {
        dict.insert(key, value);
    }
    dict.insert("root".into(), root.clone());
    if let Some(dir) = &dir {
        dict.insert("dir".into(), dir.clone());
    }
    dict.insert("name".into(), name.clone());

    Some(EvalSysCtx {
        inputs: Arc::new(LazyHash::new(dict)),
        root,
        dir,
        name,
    })
}

fn define_val(vm: &mut Vm, name: &str, value: Value) {
    let ident = SyntaxNode::leaf(SyntaxKind::Ident, name);
    vm.define(ident.cast::<ast::Ident>().unwrap(), value);
}

#[typst_macros::func(title = "Join function")]
fn join(args: &mut Args) -> SourceResult<Value> {
    let pos = args.take().to_pos();
    let mut res = PathBuf::new();
    for arg in pos {
        match arg {
            Value::Str(s) => res.push(s.as_str()),
            _ => {
                return Err(eco_format!("join argument is not a string: {arg:?}")).at(args.span);
            }
        };
    }
    Ok(Value::Str(res.to_string_lossy().into()))
}
