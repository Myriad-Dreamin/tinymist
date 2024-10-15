use std::{collections::BTreeMap, sync::LazyLock};

use reflexo::TakeAs;
use typst::foundations::{IntoValue, Module, Str, Type};

use crate::{
    docs::{convert_docs, identify_func_docs, DocStringKind},
    syntax::{find_docs_of, get_non_strict_def_target},
};

use super::*;

const DOC_VARS: u64 = 0;

impl<'a, 'w> TypeChecker<'a, 'w> {
    pub fn check_closure_docs(&mut self, root: &LinkedNode) -> Option<DocString> {
        let closure = root.cast::<ast::Closure>()?;

        // todo: cache docs capture
        // use parent of params, todo: reliable way to get the def target
        let def = get_non_strict_def_target(root.clone())?;
        let docs = find_docs_of(&self.source, def)?;

        let documenting_id = closure
            .name()
            .and_then(|n| self.get_def_id(n.span(), &to_ident_ref(root, n)?))?;
        let docstring = self.ctx.compute_docstring(docs, DocStringKind::Function)?;
        Some(docstring.take().rename_based_on(documenting_id, self))
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct DocString {
    /// The documentation of the item
    pub docs: Option<String>,
    /// The typing on definitions
    pub var_bounds: HashMap<DefId, TypeVarBounds>,
    /// The variable doc associated with the item
    pub vars: HashMap<EcoString, VarDoc>,
    /// The type of the resultant type
    pub res_ty: Option<Ty>,
}

impl DocString {
    pub fn get_var(&self, name: &str) -> Option<&VarDoc> {
        self.vars.get(name)
    }

    pub fn var_ty(&self, name: &str) -> Option<&Ty> {
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

#[derive(Debug, Clone, Default)]
pub(crate) struct VarDoc {
    pub _docs: Option<EcoString>,
    pub ty: Option<Ty>,
    pub _default: Option<EcoString>,
}

pub(crate) type VarDocs = HashMap<EcoString, VarDoc>;

pub(crate) fn compute_docstring(
    ctx: &AnalysisContext,
    docs: String,
    kind: DocStringKind,
) -> Option<DocString> {
    let checker = DocsChecker {
        ctx,
        vars: HashMap::new(),
        docs_scope: HashMap::new(),
        next_id: 0,
    };
    match kind {
        DocStringKind::Function => checker.check_closure_docs(docs),
    }
}

struct DocsChecker<'a, 'w> {
    ctx: &'a AnalysisContext<'w>,
    /// The typing on definitions
    pub vars: HashMap<DefId, TypeVarBounds>,
    docs_scope: HashMap<EcoString, Option<Ty>>,
    next_id: u32,
}

impl<'a, 'w> DocsChecker<'a, 'w> {
    pub fn check_closure_docs(mut self, docs: String) -> Option<DocString> {
        let converted = convert_docs(self.ctx.world(), &docs).ok()?;
        let converted = identify_func_docs(&converted).ok()?;
        let module = self.ctx.module_by_str(docs)?;

        let mut params = VarDocs::new();
        for param in converted.params.into_iter() {
            params.insert(
                param.name,
                VarDoc {
                    _docs: Some(param.docs),
                    ty: self.check_doc_types(&module, &param.types),
                    _default: param.default,
                },
            );
        }

        let res_ty = converted
            .return_ty
            .and_then(|ty| self.check_doc_types(&module, &ty));

        Some(DocString {
            docs: Some(converted.docs),
            var_bounds: self.vars,
            vars: params,
            res_ty,
        })
    }

    fn check_doc_types(&mut self, m: &Module, strs: &str) -> Option<Ty> {
        let mut types = vec![];
        for name in strs.split(",").map(|e| e.trim()) {
            let Some(ty) = self.check_doc_type_ident(m, name) else {
                continue;
            };
            types.push(ty);
        }

        Some(Ty::from_types(types.into_iter()))
    }

    fn check_doc_type_ident(&mut self, m: &Module, name: &str) -> Option<Ty> {
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
        builtin_ty.or_else(|| self.check_doc_type_anno(m, name))
    }

    fn check_doc_type_anno(&mut self, m: &Module, name: &str) -> Option<Ty> {
        if let Some(v) = self.docs_scope.get(name) {
            return v.clone();
        }

        let v = m.scope().get(name)?;
        log::debug!("check doc type annotation: {name:?}");
        if let Value::Content(c) = v {
            let anno = c.clone().unpack::<typst::text::RawElem>().ok()?;
            let text = anno.text().clone().into_value().cast::<Str>().ok()?;
            let code = typst::syntax::parse_code(&text.as_str().replace('\'', "θ"));
            let mut exprs = code.cast::<ast::Code>()?.exprs();
            let ret = self.check_doc_type_expr(m, exprs.next()?);
            self.docs_scope.insert(name.into(), ret.clone());
            ret
        } else {
            None
        }
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

    fn check_doc_type_expr(&mut self, m: &Module, s: ast::Expr) -> Option<Ty> {
        log::debug!("check doc type expr: {s:?}");
        match s {
            ast::Expr::Ident(i) => self.check_doc_type_ident(m, i.get().as_str()),
            ast::Expr::Closure(c) => {
                log::debug!("check doc closure annotation: {c:?}");
                let mut pos = vec![];
                let mut named = BTreeMap::new();
                let mut rest = None;

                for param in c.params().children() {
                    match param {
                        ast::Param::Pos(ast::Pattern::Normal(ast::Expr::Ident(i))) => {
                            let base_ty = self.docs_scope.get(i.get().as_str()).cloned();
                            pos.push(base_ty.flatten().unwrap_or(Ty::Any));
                        }
                        ast::Param::Pos(_) => {
                            pos.push(Ty::Any);
                        }
                        ast::Param::Named(e) => {
                            let exp = self.check_doc_type_expr(m, e.expr()).unwrap_or(Ty::Any);
                            named.insert(e.name().into(), exp);
                        }
                        // todo: spread left/right
                        ast::Param::Spread(s) => {
                            let Some(i) = s.sink_ident() else {
                                continue;
                            };
                            let name = i.get().clone();
                            let rest_ty = self
                                .docs_scope
                                .get(i.get().as_str())
                                .cloned()
                                .flatten()
                                .unwrap_or_else(|| self.generate_var(name.as_str().into()));
                            self.docs_scope.insert(name, Some(rest_ty.clone()));
                            rest = Some(rest_ty);
                        }
                    }
                }

                let body = self.check_doc_type_expr(m, c.body())?;
                let sig = SigTy::new(pos, named, rest, Some(body)).into();

                Some(Ty::Func(sig))
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