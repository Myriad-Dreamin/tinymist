//! Hybrid analysis for function calls.

use typst::syntax::SyntaxNode;

use super::{ParamSpec, Signature};
use crate::{
    analysis::{analyze_signature, PrimarySignature, SignatureTarget},
    prelude::*,
};

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
    pub signature: Signature,
    /// The mapping of arguments syntax nodes to their respective parameter
    /// info.
    pub arg_mapping: HashMap<SyntaxNode, CallParamInfo>,
}

// todo: cache call
/// Analyzes a function call.
pub fn analyze_call(
    ctx: &mut AnalysisContext,
    source: Source,
    node: LinkedNode,
) -> Option<Arc<CallInfo>> {
    trace!("func call found: {:?}", node);
    let f = node.cast::<ast::FuncCall>()?;

    let callee = f.callee();
    // todo: reduce many such patterns
    if !callee.hash() && !matches!(callee, ast::Expr::MathIdent(_)) {
        return None;
    }

    let callee_node = node.find(callee.span())?;
    Some(Arc::new(analyze_call_no_cache(
        ctx,
        source,
        callee_node,
        f.args(),
    )?))
}

/// Analyzes a function call without caching the result.
// todo: testing
pub fn analyze_call_no_cache(
    ctx: &mut AnalysisContext,
    source: Source,
    callee_node: LinkedNode,
    args: ast::Args<'_>,
) -> Option<CallInfo> {
    let signature = analyze_signature(ctx, source, SignatureTarget::Syntax(callee_node))?;
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
        out_of_arg_list: bool,
        signature: Arc<PrimarySignature>,
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
                let is_content_block =
                    self.out_of_arg_list && arg.kind() == SyntaxKind::ContentBlock;
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
                let is_content_block =
                    self.out_of_arg_list && arg.kind() == SyntaxKind::ContentBlock;
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

        fn set_out_of_arg_list(&mut self, o: bool) {
            self.out_of_arg_list = o;
        }
    }

    let mut pos_builder = PosBuilder {
        state: PosState::Init,
        out_of_arg_list: true,
        signature: signature.primary().clone(),
    };
    pos_builder.advance(&mut info, None);

    for args in signature.bindings().iter().rev() {
        for _arg in args.items.iter().filter(|arg| arg.name.is_none()) {
            pos_builder.advance(&mut info, None);
        }
    }

    for node in args.to_untyped().children() {
        match node.kind() {
            SyntaxKind::LeftParen => {
                pos_builder.set_out_of_arg_list(false);
                continue;
            }
            SyntaxKind::RightParen => {
                pos_builder.set_out_of_arg_list(true);
                continue;
            }
            _ => {}
        }
        let arg_tag = node.clone();
        let Some(arg) = node.cast::<ast::Arg>() else {
            continue;
        };

        match arg {
            ast::Arg::Named(named) => {
                let n = named.name().as_str();

                if let Some(param) = signature.primary().named.get(n) {
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

    Some(info)
}
