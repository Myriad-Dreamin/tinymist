use std::{borrow::Cow, ops::Range};

use log::debug;
use tower_lsp::lsp_types::{InlayHintKind, InlayHintLabel};
use typst::{
    foundations::{Args, Closure},
    syntax::SyntaxNode,
    util::LazyHash,
};
use typst_ts_core::typst::prelude::eco_vec;

use crate::prelude::*;

#[derive(Debug, Clone)]
pub struct InlayHintRequest {
    pub path: PathBuf,
    pub range: LspRange,
}

impl InlayHintRequest {
    pub fn request(
        self,
        world: &TypstSystemWorld,
        position_encoding: PositionEncoding,
    ) -> Option<Vec<InlayHint>> {
        let source = get_suitable_source_in_workspace(world, &self.path).ok()?;
        let range = lsp_to_typst::range(self.range, position_encoding, &source)?;

        let hints = inlay_hints(world, &source, range, position_encoding).ok()?;
        debug!(
            "got inlay hints on {source:?} => {hints:?}",
            source = source.id(),
            hints = hints.len()
        );
        if hints.is_empty() {
            let root = LinkedNode::new(source.root());
            debug!("debug root {root:#?}");
        }

        Some(hints)
    }
}

fn inlay_hints(
    world: &TypstSystemWorld,
    source: &Source,
    range: Range<usize>,
    encoding: PositionEncoding,
) -> FileResult<Vec<InlayHint>> {
    struct InlayHintWorker<'a> {
        world: &'a TypstSystemWorld,
        source: &'a Source,
        range: Range<usize>,
        encoding: PositionEncoding,
        hints: Vec<InlayHint>,
    }

    impl InlayHintWorker<'_> {
        fn analyze(&mut self, node: LinkedNode) {
            let rng = node.range();
            if rng.start >= self.range.end || rng.end <= self.range.start {
                return;
            }

            self.analyze_node(&node);

            if node.get().children().len() == 0 {
                return;
            }

            // todo: survey bad performance children?
            for child in node.children() {
                self.analyze(child);
            }
        }

        fn analyze_node(&mut self, node: &LinkedNode) -> Option<()> {
            // analyze node self
            match node.kind() {
                // Type inlay hints
                SyntaxKind::LetBinding => {
                    trace!("let binding found: {:?}", node);
                }
                // Assignment inlay hints
                SyntaxKind::Eq => {
                    trace!("assignment found: {:?}", node);
                }
                SyntaxKind::DestructAssignment => {
                    trace!("destruct assignment found: {:?}", node);
                }
                // Parameter inlay hints
                SyntaxKind::FuncCall => {
                    trace!("func call found: {:?}", node);
                    let f = node.cast::<ast::FuncCall>().unwrap();

                    let callee = f.callee();
                    // todo: reduce many such patterns
                    if !callee.hash() && !matches!(callee, ast::Expr::MathIdent(_)) {
                        return None;
                    }

                    let callee_node = node.find(callee.span())?;

                    let args = f.args();
                    let args_node = node.find(args.span())?;

                    // todo: reduce many such patterns
                    let values = analyze_expr(self.world, &callee_node);
                    let func = values.into_iter().find_map(|v| match v.0 {
                        Value::Func(f) => Some(f),
                        _ => None,
                    })?;
                    trace!("got function {func:?}");

                    let call_info = analyze_call(func, args)?;
                    trace!("got call_info {call_info:?}");

                    for arg in args.items() {
                        let Some(arg_node) = args_node.find(arg.span()) else {
                            continue;
                        };

                        let Some(info) = call_info.arg_mapping.get(&arg_node) else {
                            continue;
                        };

                        if info.param.name.is_empty() {
                            continue;
                        }

                        let pos = arg_node.range().end;
                        let lsp_pos =
                            typst_to_lsp::offset_to_position(pos, self.encoding, self.source);

                        let label = InlayHintLabel::String(if info.kind == ParamKind::Rest {
                            format!(": ..{}", info.param.name)
                        } else {
                            format!(": {}", info.param.name)
                        });

                        self.hints.push(InlayHint {
                            position: lsp_pos,
                            label,
                            kind: Some(InlayHintKind::PARAMETER),
                            text_edits: None,
                            tooltip: None,
                            padding_left: None,
                            padding_right: None,
                            data: None,
                        });
                    }

                    // todo: union signatures
                }
                SyntaxKind::Set => {
                    trace!("set rule found: {:?}", node);
                }
                _ => {}
            }

            None
        }
    }

    let mut worker = InlayHintWorker {
        world,
        source,
        range,
        encoding,
        hints: vec![],
    };

    let root = LinkedNode::new(source.root());
    worker.analyze(root);

    Ok(worker.hints)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParamKind {
    Positional,
    Named,
    Rest,
}

#[derive(Debug, Clone)]
struct CallParamInfo {
    kind: ParamKind,
    param: Arc<ParamSpec>,
    // types: EcoVec<()>,
}

#[derive(Debug, Clone)]
struct CallInfo {
    arg_mapping: HashMap<SyntaxNode, CallParamInfo>,
}

#[comemo::memoize]
fn analyze_call(func: Func, args: ast::Args<'_>) -> Option<Arc<CallInfo>> {
    Some(Arc::new(analyze_call_no_cache(func, args)?))
}

fn analyze_call_no_cache(func: Func, args: ast::Args<'_>) -> Option<CallInfo> {
    let mut info = CallInfo {
        arg_mapping: HashMap::new(),
    };

    #[derive(Debug, Clone)]
    enum ArgValue<'a> {
        Instance(Args),
        Instantiating(ast::Args<'a>),
    }

    let mut with_args = eco_vec![ArgValue::Instantiating(args)];

    use typst::foundations::func::Repr;
    let mut func = func;
    while let Repr::With(f) = func.inner() {
        with_args.push(ArgValue::Instance(f.1.clone()));
        func = f.0.clone();
    }

    let signature = analyze_signature(func);
    trace!("got signature {signature:?}");

    enum PosState {
        Init,
        Pos(usize),
        Variadic,
        Final,
    }

    struct PosBuilder {
        state: PosState,
        signature: Arc<Signature>,
    }

    impl PosBuilder {
        fn advance(&mut self, info: &mut CallInfo, arg: Option<SyntaxNode>) {
            let (kind, param) = match self.state {
                PosState::Init => {
                    if !self.signature.pos.is_empty() {
                        self.state = PosState::Pos(0);
                    } else if self.signature.rest.is_some() {
                        self.state = PosState::Variadic;
                    } else {
                        self.state = PosState::Final;
                    }

                    return;
                }
                PosState::Pos(i) => {
                    if i + 1 < self.signature.pos.len() {
                        self.state = PosState::Pos(i + 1);
                    } else if self.signature.rest.is_some() {
                        self.state = PosState::Variadic;
                    } else {
                        self.state = PosState::Final;
                    }

                    (ParamKind::Positional, &self.signature.pos[i])
                }
                PosState::Variadic => (ParamKind::Rest, self.signature.rest.as_ref().unwrap()),
                PosState::Final => return,
            };

            if let Some(arg) = arg {
                info.arg_mapping.insert(
                    arg,
                    CallParamInfo {
                        kind,
                        param: param.clone(),
                        // types: eco_vec![],
                    },
                );
            }
        }

        fn advance_rest(&mut self, info: &mut CallInfo, arg: Option<SyntaxNode>) {
            match self.state {
                PosState::Init => unreachable!(),
                // todo: not precise
                PosState::Pos(..) => {
                    if self.signature.rest.is_some() {
                        self.state = PosState::Variadic;
                    } else {
                        self.state = PosState::Final;
                    }
                }
                PosState::Variadic => {}
                PosState::Final => return,
            };

            let Some(rest) = self.signature.rest.as_ref() else {
                return;
            };

            if let Some(arg) = arg {
                info.arg_mapping.insert(
                    arg,
                    CallParamInfo {
                        kind: ParamKind::Rest,
                        param: rest.clone(),
                        // types: eco_vec![],
                    },
                );
            }
        }
    }

    let mut pos_builder = PosBuilder {
        state: PosState::Init,
        signature: signature.clone(),
    };
    pos_builder.advance(&mut info, None);

    for arg in with_args.iter().rev() {
        match arg {
            ArgValue::Instance(args) => {
                for _ in args.items.iter().filter(|arg| arg.name.is_none()) {
                    pos_builder.advance(&mut info, None);
                }
            }
            ArgValue::Instantiating(args) => {
                for arg in args.items() {
                    let arg_tag = arg.to_untyped().clone();
                    match arg {
                        ast::Arg::Named(named) => {
                            let n = named.name().as_str();

                            if let Some(param) = signature.named.get(n) {
                                info.arg_mapping.insert(
                                    arg_tag,
                                    CallParamInfo {
                                        kind: ParamKind::Named,
                                        param: param.clone(),
                                        // types: eco_vec![],
                                    },
                                );
                            }
                        }
                        ast::Arg::Pos(..) => {
                            pos_builder.advance(&mut info, Some(arg_tag));
                        }
                        ast::Arg::Spread(..) => pos_builder.advance_rest(&mut info, Some(arg_tag)),
                    }
                }
            }
        }
    }

    Some(info)
}

/// Describes a function parameter.
#[derive(Debug, Clone)]
pub struct ParamSpec {
    /// The parameter's name.
    pub name: Cow<'static, str>,
    /// Creates an instance of the parameter's default value.
    pub default: Option<fn() -> Value>,
    /// Is the parameter positional?
    pub positional: bool,
    /// Is the parameter named?
    ///
    /// Can be true even if `positional` is true if the parameter can be given
    /// in both variants.
    pub named: bool,
    /// Can the parameter be given any number of times?
    pub variadic: bool,
}

impl ParamSpec {
    fn from_static(s: &ParamInfo) -> Arc<Self> {
        Arc::new(Self {
            name: Cow::Borrowed(s.name),
            default: s.default,
            positional: s.positional,
            named: s.named,
            variadic: s.variadic,
        })
    }
}

#[derive(Debug, Clone)]
struct Signature {
    pos: Vec<Arc<ParamSpec>>,
    named: HashMap<Cow<'static, str>, Arc<ParamSpec>>,
    rest: Option<Arc<ParamSpec>>,
    _broken: bool,
}

#[comemo::memoize]
fn analyze_signature(func: Func) -> Arc<Signature> {
    use typst::foundations::func::Repr;
    let params = match func.inner() {
        Repr::With(..) => unreachable!(),
        Repr::Closure(c) => analyze_closure_signature(c.clone()),
        Repr::Element(..) | Repr::Native(..) => {
            let params = func.params().unwrap();
            params.iter().map(ParamSpec::from_static).collect()
        }
    };

    let mut pos = vec![];
    let mut named = HashMap::new();
    let mut rest = None;
    let mut broken = false;

    for param in params.into_iter() {
        if param.named {
            named.insert(param.name.clone(), param.clone());
        }

        if param.variadic {
            if rest.is_some() {
                broken = true;
            } else {
                rest = Some(param.clone());
            }
        }

        if param.positional {
            pos.push(param);
        }
    }

    Arc::new(Signature {
        pos,
        named,
        rest,
        _broken: broken,
    })
}

fn analyze_closure_signature(c: Arc<LazyHash<Closure>>) -> Vec<Arc<ParamSpec>> {
    let mut params = vec![];

    trace!("closure signature for: {:?}", c.node.kind());

    let closure = &c.node;
    let closure_ast = match closure.kind() {
        SyntaxKind::Closure => closure.cast::<ast::Closure>().unwrap(),
        _ => return params,
    };

    for param in closure_ast.params().children() {
        match param {
            ast::Param::Pos(ast::Pattern::Placeholder(..)) => {
                params.push(Arc::new(ParamSpec {
                    name: Cow::Borrowed("_"),
                    default: None,
                    positional: true,
                    named: false,
                    variadic: false,
                }));
            }
            ast::Param::Pos(e) => {
                // todo: destructing
                let name = e.bindings();
                if name.len() != 1 {
                    continue;
                }
                let name = name[0].as_str();

                params.push(Arc::new(ParamSpec {
                    name: Cow::Owned(name.to_owned()),
                    default: None,
                    positional: true,
                    named: false,
                    variadic: false,
                }));
            }
            ast::Param::Named(n) => {
                params.push(Arc::new(ParamSpec {
                    name: Cow::Owned(n.name().as_str().to_owned()),
                    default: None,
                    positional: false,
                    named: true,
                    variadic: false,
                }));
            }
            ast::Param::Spread(n) => {
                let ident = n.sink_ident().map(|e| e.as_str());
                params.push(Arc::new(ParamSpec {
                    name: Cow::Owned(ident.unwrap_or_default().to_owned()),
                    default: None,
                    positional: false,
                    named: true,
                    variadic: false,
                }));
            }
        }
    }

    params
}
