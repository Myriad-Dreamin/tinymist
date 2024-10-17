use std::sync::OnceLock;

use reflexo::TakeAs;
use typst::foundations::{IntoValue, Module, Str, Type};

use super::*;
use crate::{
    adt::snapshot_map::SnapshotMap,
    docs::{
        convert_docs, identify_func_docs, identify_var_docs, DocStringKind, UntypedSymbolDocs,
        VarDocsT,
    },
    syntax::{find_docs_of, get_non_strict_def_target},
};

const DOC_VARS: u64 = 0;

impl<'a, 'w> TypeChecker<'a, 'w> {
    pub fn check_var_docs(&mut self, root: &LinkedNode) -> Option<Arc<DocString>> {
        let lb = root.cast::<ast::LetBinding>()?;
        let first = lb.kind().bindings();
        let documenting_id = first
            .first()
            .and_then(|n| self.get_def_id(n.span(), &to_ident_ref(root, *n)?))?;

        self.check_docstring(root, DocStringKind::Variable, documenting_id)
    }

    pub fn check_docstring(
        &mut self,
        root: &LinkedNode,
        kind: DocStringKind,
        base_id: DefId,
    ) -> Option<Arc<DocString>> {
        // todo: cache docs capture
        // use parent of params, todo: reliable way to get the def target
        let def = get_non_strict_def_target(root.clone())?;
        let docs = find_docs_of(&self.source, def)?;

        let docstring = self.ctx.compute_docstring(root.span().id()?, docs, kind)?;
        Some(Arc::new(docstring.take().rename_based_on(base_id, self)))
    }
}

/// The documentation string of an item
#[derive(Debug, Clone, Default)]
pub struct DocString {
    /// The documentation of the item
    pub docs: Option<EcoString>,
    /// The typing on definitions
    pub var_bounds: HashMap<DefId, TypeVarBounds>,
    /// The variable doc associated with the item
    pub vars: BTreeMap<StrRef, VarDoc>,
    /// The type of the resultant type
    pub res_ty: Option<Ty>,
}

impl DocString {
    /// Get the documentation of a variable associated with the item
    pub fn get_var(&self, name: &StrRef) -> Option<&VarDoc> {
        self.vars.get(name)
    }

    /// Get the type of a variable associated with the item
    pub fn var_ty(&self, name: &StrRef) -> Option<&Ty> {
        self.get_var(name).and_then(|v| v.ty.as_ref())
    }

    fn rename_based_on(self, documenting_id: DefId, base: &mut TypeChecker) -> DocString {
        let DocString {
            docs,
            var_bounds,
            vars,
            mut res_ty,
        } = self;
        let mut renamer = IdRenamer {
            base,
            var_bounds: &var_bounds,
            base_id: documenting_id,
            offset: DOC_VARS,
        };
        let mut vars = vars;
        for (_name, doc) in vars.iter_mut() {
            if let Some(ty) = &mut doc.ty {
                if let Some(mutated) = ty.mutate(true, &mut renamer) {
                    *ty = mutated;
                }
            }
        }
        if let Some(ty) = res_ty.as_mut() {
            if let Some(mutated) = ty.mutate(true, &mut renamer) {
                *ty = mutated;
            }
        }
        DocString {
            docs,
            var_bounds,
            vars,
            res_ty,
        }
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
    pub fn to_untyped(&self) -> Arc<UntypedSymbolDocs> {
        Arc::new(UntypedSymbolDocs::Variable(VarDocsT {
            docs: self.docs.clone(),
            return_ty: (),
            def_docs: OnceLock::new(),
        }))
    }
}

pub(crate) fn compute_docstring(
    ctx: &AnalysisContext,
    fid: TypstFileId,
    docs: String,
    kind: DocStringKind,
) -> Option<DocString> {
    let checker = DocsChecker {
        fid,
        ctx,
        vars: HashMap::new(),
        globals: HashMap::default(),
        locals: SnapshotMap::default(),
        next_id: 0,
    };
    match kind {
        DocStringKind::Function => checker.check_func_docs(docs),
        DocStringKind::Variable => checker.check_var_docs(docs),
        DocStringKind::Module => None,
        DocStringKind::Constant => None,
        DocStringKind::Struct => None,
        DocStringKind::Reference => None,
    }
}

struct DocsChecker<'a, 'w> {
    fid: TypstFileId,
    ctx: &'a AnalysisContext<'w>,
    /// The typing on definitions
    vars: HashMap<DefId, TypeVarBounds>,
    globals: HashMap<EcoString, Option<Ty>>,
    locals: SnapshotMap<EcoString, Ty>,
    next_id: u32,
}

impl<'a, 'w> DocsChecker<'a, 'w> {
    pub fn check_func_docs(mut self, docs: String) -> Option<DocString> {
        let converted = convert_docs(self.ctx.world(), &docs).ok()?;
        let converted = identify_func_docs(&converted).ok()?;
        let module = self.ctx.module_by_str(docs)?;

        let mut params = BTreeMap::new();
        for param in converted.params.into_iter() {
            params.insert(
                param.name.into(),
                VarDoc {
                    docs: param.docs,
                    ty: self.check_type_strings(&module, &param.types),
                },
            );
        }

        let res_ty = converted
            .return_ty
            .and_then(|ty| self.check_type_strings(&module, &ty));

        Some(DocString {
            docs: Some(converted.docs),
            var_bounds: self.vars,
            vars: params,
            res_ty,
        })
    }

    pub fn check_var_docs(mut self, docs: String) -> Option<DocString> {
        let converted = convert_docs(self.ctx.world(), &docs).ok()?;
        let converted = identify_var_docs(converted).ok()?;
        let module = self.ctx.module_by_str(docs)?;

        let res_ty = converted
            .return_ty
            .and_then(|ty| self.check_type_strings(&module, &ty.0));

        Some(DocString {
            docs: Some(converted.docs),
            var_bounds: self.vars,
            vars: BTreeMap::new(),
            res_ty,
        })
    }

    fn generate_var(&mut self, name: StrRef) -> Ty {
        self.next_id += 1;
        let encoded = DefId(self.next_id as u64);
        log::debug!("generate var {name:?} {encoded:?}");
        let bounds = TypeVarBounds::new(TypeVar { name, def: encoded }, TypeBounds::default());
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

struct IdRenamer<'a, 'b, 'w> {
    base: &'a mut TypeChecker<'b, 'w>,
    var_bounds: &'a HashMap<DefId, TypeVarBounds>,
    base_id: DefId,
    offset: u64,
}

impl<'a, 'b, 'w> TyMutator for IdRenamer<'a, 'b, 'w> {
    fn mutate(&mut self, ty: &Ty, pol: bool) -> Option<Ty> {
        match ty {
            Ty::Var(v) => Some(self.base.copy_based_on(
                self.var_bounds.get(&v.def).unwrap(),
                self.offset,
                self.base_id,
            )),
            ty => self.mutate_rec(ty, pol),
        }
    }
}
