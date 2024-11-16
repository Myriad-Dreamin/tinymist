use std::{
    collections::BTreeMap,
    ops::Deref,
    sync::{LazyLock, OnceLock},
};

use ecow::eco_format;
use typst::foundations::{IntoValue, Module, Str, Type};

use crate::{
    adt::snapshot_map::SnapshotMap,
    analysis::SharedContext,
    docs::{convert_docs, identify_pat_docs, identify_tidy_module_docs, UntypedDefDocs, VarDocsT},
    prelude::*,
    syntax::{Decl, DefKind},
    ty::{
        BuiltinTy, InsTy, Interned, PackageId, SigTy, StrRef, Ty, TypeBounds, TypeVar,
        TypeVarBounds,
    },
};

use super::DeclExpr;

/// The documentation string of an item
#[derive(Debug, Clone, Default)]
pub struct DocString {
    /// The documentation of the item
    pub docs: Option<EcoString>,
    /// The typing on definitions
    pub var_bounds: HashMap<DeclExpr, TypeVarBounds>,
    /// The variable doc associated with the item
    pub vars: BTreeMap<StrRef, VarDoc>,
    /// The type of the resultant type
    pub res_ty: Option<Ty>,
}

impl DocString {
    pub fn as_var(&self) -> VarDoc {
        VarDoc {
            docs: self.docs.clone().unwrap_or_default(),
            ty: self.res_ty.clone(),
        }
    }

    /// Get the documentation of a variable associated with the item
    pub fn get_var(&self, name: &StrRef) -> Option<&VarDoc> {
        self.vars.get(name)
    }

    /// Get the type of a variable associated with the item
    pub fn var_ty(&self, name: &StrRef) -> Option<&Ty> {
        self.get_var(name).and_then(|v| v.ty.as_ref())
    }
}

/// The documentation string of a variable associated with some item.
#[derive(Debug, Clone, Default)]
pub struct VarDoc {
    /// The documentation of the variable
    pub docs: EcoString,
    /// The type of the variable
    pub ty: Option<Ty>,
}

impl VarDoc {
    /// Convert the variable doc to an untyped version
    pub fn to_untyped(&self) -> Arc<UntypedDefDocs> {
        Arc::new(UntypedDefDocs::Variable(VarDocsT {
            docs: self.docs.clone(),
            return_ty: (),
            def_docs: OnceLock::new(),
        }))
    }
}

pub(crate) fn compute_docstring(
    ctx: &Arc<SharedContext>,
    fid: TypstFileId,
    docs: String,
    kind: DefKind,
) -> Option<DocString> {
    let checker = DocsChecker {
        fid,
        ctx,
        vars: HashMap::new(),
        globals: HashMap::default(),
        locals: SnapshotMap::default(),
        next_id: 0,
    };
    use DefKind::*;
    match kind {
        Function | Variable => checker.check_pat_docs(docs),
        Module => checker.check_module_docs(docs),
        Constant | Struct | Reference => None,
    }
}

struct DocsChecker<'a> {
    fid: TypstFileId,
    ctx: &'a Arc<SharedContext>,
    /// The typing on definitions
    vars: HashMap<DeclExpr, TypeVarBounds>,
    globals: HashMap<EcoString, Option<Ty>>,
    locals: SnapshotMap<EcoString, Ty>,
    next_id: u32,
}

static EMPTY_MODULE: LazyLock<Module> =
    LazyLock::new(|| Module::new("stub", typst::foundations::Scope::new()));

impl<'a> DocsChecker<'a> {
    pub fn check_pat_docs(mut self, docs: String) -> Option<DocString> {
        let converted =
            convert_docs(self.ctx, &docs).and_then(|converted| identify_pat_docs(&converted));

        let converted = match Self::fallback_docs(converted, &docs) {
            Ok(c) => c,
            Err(e) => return Some(e),
        };

        let module = self.ctx.module_by_str(docs);
        let module = module.as_ref().unwrap_or(EMPTY_MODULE.deref());

        let mut params = BTreeMap::new();
        for param in converted.params.into_iter() {
            params.insert(
                param.name.into(),
                VarDoc {
                    docs: self.ctx.remove_html(param.docs),
                    ty: self.check_type_strings(module, &param.types),
                },
            );
        }

        let res_ty = converted
            .return_ty
            .and_then(|ty| self.check_type_strings(module, &ty));

        Some(DocString {
            docs: Some(self.ctx.remove_html(converted.docs)),
            var_bounds: self.vars,
            vars: params,
            res_ty,
        })
    }

    pub fn check_module_docs(self, docs: String) -> Option<DocString> {
        let converted = convert_docs(self.ctx, &docs).and_then(identify_tidy_module_docs);

        let converted = match Self::fallback_docs(converted, &docs) {
            Ok(c) => c,
            Err(e) => return Some(e),
        };

        Some(DocString {
            docs: Some(self.ctx.remove_html(converted.docs)),
            var_bounds: self.vars,
            vars: BTreeMap::new(),
            res_ty: None,
        })
    }

    fn fallback_docs<T>(converted: Result<T, EcoString>, docs: &str) -> Result<T, DocString> {
        match converted {
            Ok(c) => Ok(c),
            Err(e) => {
                let e = e.replace("`", "\\`");
                let max_consecutive_backticks = docs
                    .chars()
                    .fold((0, 0), |(max, count), c| {
                        if c == '`' {
                            (max.max(count + 1), count + 1)
                        } else {
                            (max, 0)
                        }
                    })
                    .0;
                let backticks = "`".repeat((max_consecutive_backticks + 1).max(3));
                let fallback_docs = eco_format!(
                    "```\nfailed to parse docs: {e}\n```\n\n{backticks}typ\n{docs}\n{backticks}\n"
                );
                Err(DocString {
                    docs: Some(fallback_docs),
                    var_bounds: HashMap::new(),
                    vars: BTreeMap::new(),
                    res_ty: None,
                })
            }
        }
    }

    fn generate_var(&mut self, name: StrRef) -> Ty {
        self.next_id += 1;
        let encoded = Interned::new(Decl::generated(DefId(self.next_id as u64)));
        log::debug!("generate var {name:?} {encoded:?}");
        let var = TypeVar {
            name,
            def: encoded.clone(),
        };
        let bounds = TypeVarBounds::new(var, TypeBounds::default());
        let var = bounds.as_type();
        self.vars.insert(encoded, bounds);
        var
    }

    fn check_type_strings(&mut self, m: &Module, strs: &str) -> Option<Ty> {
        let mut types = vec![];
        for name in strs.split(",").map(|e| e.trim()) {
            let Some(ty) = self.check_type_ident(m, name) else {
                continue;
            };
            types.push(ty);
        }

        Some(Ty::from_types(types.into_iter()))
    }

    fn check_type_ident(&mut self, m: &Module, name: &str) -> Option<Ty> {
        static TYPE_REPRS: LazyLock<HashMap<&'static str, Ty>> = LazyLock::new(|| {
            let values = Vec::from_iter(
                [
                    Value::None,
                    Value::Auto,
                    // Value::Bool(Default::default()),
                    Value::Int(Default::default()),
                    Value::Float(Default::default()),
                    Value::Length(Default::default()),
                    Value::Angle(Default::default()),
                    Value::Ratio(Default::default()),
                    Value::Relative(Default::default()),
                    Value::Fraction(Default::default()),
                    Value::Str(Default::default()),
                ]
                .map(|v| v.ty())
                .into_iter()
                .chain([
                    Type::of::<typst::visualize::Color>(),
                    Type::of::<typst::visualize::Gradient>(),
                    Type::of::<typst::visualize::Pattern>(),
                    Type::of::<typst::symbols::Symbol>(),
                    Type::of::<typst::foundations::Version>(),
                    Type::of::<typst::foundations::Bytes>(),
                    Type::of::<typst::foundations::Label>(),
                    Type::of::<typst::foundations::Datetime>(),
                    Type::of::<typst::foundations::Duration>(),
                    Type::of::<typst::foundations::Content>(),
                    Type::of::<typst::foundations::Styles>(),
                    Type::of::<typst::foundations::Array>(),
                    Type::of::<typst::foundations::Dict>(),
                    Type::of::<typst::foundations::Func>(),
                    Type::of::<typst::foundations::Args>(),
                    Type::of::<typst::foundations::Type>(),
                    Type::of::<typst::foundations::Module>(),
                ]),
            );

            let shorts = values
                .clone()
                .into_iter()
                .map(|ty| (ty.short_name(), Ty::Builtin(BuiltinTy::Type(ty))));
            let longs = values
                .into_iter()
                .map(|ty| (ty.long_name(), Ty::Builtin(BuiltinTy::Type(ty))));
            let builtins = [
                ("any", Ty::Any),
                ("bool", Ty::Boolean(None)),
                ("boolean", Ty::Boolean(None)),
                ("false", Ty::Boolean(Some(false))),
                ("true", Ty::Boolean(Some(true))),
            ];
            HashMap::from_iter(shorts.chain(longs).chain(builtins))
        });

        let builtin_ty = TYPE_REPRS.get(name).cloned();
        builtin_ty
            .or_else(|| self.locals.get(name).cloned())
            .or_else(|| self.check_type_annotation(m, name))
    }

    fn check_type_annotation(&mut self, m: &Module, name: &str) -> Option<Ty> {
        if let Some(v) = self.globals.get(name) {
            return v.clone();
        }

        let v = m.scope().get(name)?;
        log::debug!("check doc type annotation: {name:?}");
        if let Value::Content(c) = v {
            let annotated = c.clone().unpack::<typst::text::RawElem>().ok()?;
            let text = annotated.text().clone().into_value().cast::<Str>().ok()?;
            let code = typst::syntax::parse_code(&text.as_str().replace('\'', "θ"));
            let mut exprs = code.cast::<ast::Code>()?.exprs();
            let ret = self.check_type_expr(m, exprs.next()?);
            self.globals.insert(name.into(), ret.clone());
            ret
        } else {
            None
        }
    }

    fn check_type_expr(&mut self, m: &Module, s: ast::Expr) -> Option<Ty> {
        log::debug!("check doc type expr: {s:?}");
        match s {
            ast::Expr::Ident(i) => self.check_type_ident(m, i.get().as_str()),
            ast::Expr::None(_)
            | ast::Expr::Auto(_)
            | ast::Expr::Bool(..)
            | ast::Expr::Int(..)
            | ast::Expr::Float(..)
            | ast::Expr::Numeric(..)
            | ast::Expr::Str(..) => SharedContext::const_eval(s).map(|v| Ty::Value(InsTy::new(v))),
            ast::Expr::Binary(b) => {
                let mut components = Vec::with_capacity(2);
                components.push(self.check_type_expr(m, b.lhs())?);

                let mut expr = b.rhs();
                while let ast::Expr::Binary(b) = expr {
                    if b.op() != ast::BinOp::Or {
                        break;
                    }

                    components.push(self.check_type_expr(m, b.lhs())?);
                    expr = b.rhs();
                }

                components.push(self.check_type_expr(m, expr)?);
                Some(Ty::from_types(components.into_iter()))
            }
            ast::Expr::FuncCall(c) => match c.callee() {
                ast::Expr::Ident(i) => {
                    let name = i.get().as_str();
                    match name {
                        "array" => Some({
                            let ast::Arg::Pos(pos) = c.args().items().next()? else {
                                return None;
                            };

                            Ty::Array(self.check_type_expr(m, pos)?.into())
                        }),
                        "tag" => Some({
                            let ast::Arg::Pos(ast::Expr::Str(s)) = c.args().items().next()? else {
                                return None;
                            };
                            let pkg_id = PackageId::try_from(self.fid).ok();
                            Ty::Builtin(BuiltinTy::Tag(Box::new((
                                s.get().into(),
                                pkg_id.map(From::from),
                            ))))
                        }),
                        _ => None,
                    }
                }
                _ => None,
            },
            ast::Expr::Closure(c) => {
                log::debug!("check doc closure annotation: {c:?}");
                let mut pos = vec![];
                let mut named = BTreeMap::new();
                let mut rest = None;
                let snap = self.locals.snapshot();

                let sig = None.or_else(|| {
                    for param in c.params().children() {
                        match param {
                            ast::Param::Pos(ast::Pattern::Normal(ast::Expr::Ident(i))) => {
                                let name = i.get().clone();
                                let base_ty = self.generate_var(name.as_str().into());
                                self.locals.insert(name, base_ty.clone());
                                pos.push(base_ty);
                            }
                            ast::Param::Pos(_) => {
                                pos.push(Ty::Any);
                            }
                            ast::Param::Named(e) => {
                                let exp = self.check_type_expr(m, e.expr()).unwrap_or(Ty::Any);
                                named.insert(e.name().into(), exp);
                            }
                            // todo: spread left/right
                            ast::Param::Spread(s) => {
                                let Some(i) = s.sink_ident() else {
                                    continue;
                                };
                                let name = i.get().clone();
                                let rest_ty = self.generate_var(name.as_str().into());
                                self.locals.insert(name, rest_ty.clone());
                                rest = Some(rest_ty);
                            }
                        }
                    }

                    let body = self.check_type_expr(m, c.body())?;
                    let sig = SigTy::new(pos.into_iter(), named, None, rest, Some(body)).into();

                    Some(Ty::Func(sig))
                });

                self.locals.rollback_to(snap);
                sig
            }
            ast::Expr::Dict(d) => {
                log::debug!("check doc dict annotation: {d:?}");
                None
            }
            _ => None,
        }
    }
}
