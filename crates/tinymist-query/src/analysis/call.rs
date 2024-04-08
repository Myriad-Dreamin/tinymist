//! Hybrid analysis for function calls.

use ecow::eco_vec;
use typst::{foundations::Args, syntax::SyntaxNode};

use super::{analyze_signature, ParamSpec, Signature};
use crate::prelude::*;

/// Describes kind of a parameter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamKind {
    /// A positional parameter.
    Positional,
    /// A named parameter.
    Named,
    /// A rest (spread) parameter.
    Rest,
}

/// Describes a function call parameter.
#[derive(Debug, Clone)]
pub struct CallParamInfo {
    /// The parameter's kind.
    pub kind: ParamKind,
    /// Whether the parameter is a content block.
    pub is_content_block: bool,
    /// The parameter's specification.
    pub param: Arc<ParamSpec>,
    // types: EcoVec<()>,
}

/// Describes a function call.
#[derive(Debug, Clone)]
pub struct CallInfo {
    /// The called function's signature.
    pub signature: Arc<Signature>,
    /// The mapping of arguments syntax nodes to their respective parameter
    /// info.
    pub arg_mapping: HashMap<SyntaxNode, CallParamInfo>,
}

// todo: cache call
/// Analyzes a function call.
pub fn analyze_call(
    ctx: &mut AnalysisContext,
    func: Func,
    args: ast::Args<'_>,
) -> Option<Arc<CallInfo>> {
    Some(Arc::new(analyze_call_no_cache(ctx, func, args)?))
}

/// Analyzes a function call without caching the result.
pub fn analyze_call_no_cache(
    ctx: &mut AnalysisContext,
    func: Func,
    args: ast::Args<'_>,
) -> Option<CallInfo> {
    let _ = ctx;
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

    let signature = analyze_signature(ctx, func);
    trace!("got signature {signature:?}");

    let mut info = CallInfo {
        arg_mapping: HashMap::new(),
        signature: signature.clone(),
    };

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
                // todo: process desugar
                let is_content_block = arg.kind() == SyntaxKind::ContentBlock;
                info.arg_mapping.insert(
                    arg,
                    CallParamInfo {
                        kind,
                        is_content_block,
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
                // todo: process desugar
                let is_content_block = arg.kind() == SyntaxKind::ContentBlock;
                info.arg_mapping.insert(
                    arg,
                    CallParamInfo {
                        kind: ParamKind::Rest,
                        is_content_block,
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
                                        is_content_block: false,
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
