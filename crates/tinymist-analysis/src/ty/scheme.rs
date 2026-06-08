#![allow(unused)]

use core::fmt;
use std::{collections::VecDeque, path::Path, sync::OnceLock};

use comemo::Track;
use tinymist_world::args;
use typst::{
    engine::Engine,
    foundations::{
        Args, Closure, ClosureNode, Context, Func, Repr as ValueRepr, Str, Value, func::Repr,
    },
    syntax::{Source, Span, ast},
};

use super::Ty;
use crate::{
    func_signature,
    syntax::Decl,
    ty::{
        BuiltinTy, InsTy, Interned, ParamAttrs, ParamTy, RecordTy, SigTy, TypeInfo, TypeVar,
        is_plain_value,
    },
};

enum TyMark {
    Norm(Ty),
    Sig(Ty),
    Any,
}

impl TyMark {
    fn ty(self) -> Ty {
        match self {
            TyMark::Any => Ty::Any,
            TyMark::Norm(ty) => ty,
            TyMark::Sig(ty) => ty,
        }
    }
}

pub struct TySchemeWorker<'a> {
    engine: Engine<'a>,
    scheme: &'a mut TypeInfo,
    self_stack: Vec<SelfTyCtx>,
    tv_stack: Vec<TvCtx>,
    fresh_vars: u64,
}

struct SelfTyCtx {
    name: Interned<str>,
    ty: Option<Ty>,
}

#[derive(Default)]
struct TvCtx {
    explicit: VecDeque<Ty>,
    vars: Vec<((Interned<str>, Span), Ty)>,
}

impl TySchemeWorker<'_> {
    fn is_typing_item(&self, v: &Value) -> bool {
        matches!(v, Value::Func(f) if f.span().id().is_some_and(|s| s.vpath().as_rooted_path() == Path::new("/typings.typ")))
    }
}

impl TySchemeWorker<'_> {
    pub fn define(&mut self, k: &str, v: &Value) -> Ty {
        self.define_with_mark(k, v).ty()
    }

    fn define_with_mark(&mut self, k: &str, v: &Value) -> TyMark {
        if !self.is_typing_item(v) {
            self.define_value(v)
        } else {
            self.define_typing_value(k, v)
        }
    }

    fn define_typing_value(&mut self, k: &str, v: &Value) -> TyMark {
        let closure = match v {
            Value::Func(f) => f,
            _ => return TyMark::Any,
        };

        let with = match closure.inner() {
            Repr::With(c) => c,
            _ => return TyMark::Any,
        };

        self.define_typing(k, &with.0, with.1.clone())
    }

    fn define_typing(&mut self, k: &str, func: &Func, mut args: Args) -> TyMark {
        match func.inner() {
            Repr::Native(..) | Repr::Element(..) | Repr::Plugin(..) => TyMark::Any,
            Repr::With(with) => {
                args.items = with.1.items.iter().cloned().chain(args.items).collect();
                self.define_typing(k, &with.0, args)
            }
            Repr::Closure(closure) => self.ty_cons(k, args).unwrap_or(TyMark::Any),
        }
    }

    fn ty_cons(&mut self, k: &str, mut args: Args) -> Option<TyMark> {
        crate::log_debug_ct!("ty_cons({k}) args: {args:?}");
        let kind = args.named::<Str>("kind").ok()??;
        let ty = match kind.as_str() {
            "var" => self.define(k, &args.eat::<Value>().ok()??),
            "union" => {
                let mut candidates = vec![];
                for arg in args.all::<Value>().ok()? {
                    candidates.push(self.define(k, &arg));
                }

                if candidates.is_empty() {
                    return Some(TyMark::Any);
                }
                if candidates.len() == 1 {
                    return Some(TyMark::Norm(candidates.pop().unwrap()));
                }

                return Some(TyMark::Norm(Ty::Union(Interned::new(candidates))));
            }
            "sig" => {
                let mut candidates = vec![];
                for arg in args.all::<Value>().ok()? {
                    candidates.push(self.define(k, &arg));
                }

                if candidates.is_empty() {
                    return Some(TyMark::Any);
                }
                if candidates.len() == 1 {
                    return Some(TyMark::Norm(candidates.pop().unwrap()));
                }

                return Some(TyMark::Sig(Ty::Union(Interned::new(candidates))));
            }
            "tv" => {
                let name = args.eat::<Str>().ok()??;
                self.type_var(name.as_str(), args.span)
            }
            "rec" => {
                let rec_name = args
                    .named::<Str>("name")
                    .ok()
                    .flatten()
                    .map(|name| Interned::new_str(name.as_str()))
                    .unwrap_or_else(|| Interned::new_str(k));
                let scope = args.named::<Value>("scope").ok()??;
                let Value::Dict(scope) = scope else {
                    return Some(TyMark::Any);
                };
                let explicit = args
                    .all::<Value>()
                    .ok()?
                    .into_iter()
                    .map(|value| self.define(k, &value))
                    .collect();
                let self_ty = args
                    .named::<Value>("self")
                    .ok()
                    .flatten()
                    .map(|value| self.define(k, &value));
                self.self_stack.push(SelfTyCtx {
                    name: rec_name,
                    ty: self_ty,
                });
                self.tv_stack.push(TvCtx {
                    explicit,
                    vars: vec![],
                });
                let fields = scope
                    .iter()
                    .map(|(name, value)| (name.as_str().into(), self.define(name.as_str(), value)))
                    .collect();
                self.tv_stack.pop();
                self.self_stack.pop();
                Ty::Dict(RecordTy::new(fields))
            }
            "self" => {
                let args = args
                    .all::<Value>()
                    .ok()?
                    .into_iter()
                    .map(|value| self.define(k, &value))
                    .collect::<Vec<_>>();
                let ty = self.self_ty(args);
                if k == "self" {
                    Ty::Param(Interned::new(ParamTy {
                        name: k.into(),
                        docs: None,
                        default: None,
                        required: true,
                        ty,
                        attrs: ParamAttrs::positional(),
                    }))
                } else {
                    ty
                }
            }
            "arr" => {
                let ty = self.define(k, &args.eat::<Value>().ok()??);
                Ty::Array(Interned::new(ty))
            }
            "dict" => {
                let values = args.all::<Value>().ok()?;
                let val_ty = values.get(1).map(|v| self.define(k, v)).unwrap_or(Ty::Any);
                Ty::Dict(RecordTy::new(vec![("..".into(), val_ty)]))
            }
            "record" => {
                let fields = args
                    .to_named()
                    .into_iter()
                    .map(|(name, value)| (name.as_str().into(), self.define(name.as_str(), &value)))
                    .collect();
                Ty::Dict(RecordTy::new(fields))
            }
            "tuple" => {
                let values = args.all::<Value>().ok()?;
                let values = values
                    .into_iter()
                    .map(|v| self.define(k, &v))
                    .collect::<Vec<_>>();
                Ty::Tuple(Interned::new(values))
            }
            "pos" => self.term_param(k, args, ParamAttrs::positional())?,
            "named" => self.term_param(k, args, ParamAttrs::named())?,
            "pos-named" => self.term_param(k, args, ParamAttrs::pos_named())?,
            "rest" => self.term_param(k, args, ParamAttrs::variadic())?,
            _ => Ty::Any,
        };
        Some(TyMark::Norm(ty))
    }

    fn type_var(&mut self, name: &str, span: Span) -> Ty {
        let key = (Interned::new_str(name), span);
        let Some(idx) = self.tv_stack.len().checked_sub(1) else {
            return Ty::Var(TypeVar::new(key.0, Interned::new(Decl::lit_at(name, span))));
        };

        if let Some((_, ty)) = self.tv_stack[idx]
            .vars
            .iter()
            .find(|(existing, _)| *existing == key)
        {
            return ty.clone();
        }

        let ty = if let Some(explicit) = self.tv_stack[idx].explicit.pop_front() {
            explicit
        } else {
            self.fresh_type_var(name)
        };
        self.tv_stack[idx].vars.push((key, ty.clone()));
        ty
    }

    fn fresh_type_var(&mut self, name: &str) -> Ty {
        self.fresh_vars += 1;
        Ty::Var(TypeVar::new(
            name.into(),
            Interned::new(Decl::generated(tinymist_std::DefId(self.fresh_vars))),
        ))
    }

    fn self_ty(&mut self, args: Vec<Ty>) -> Ty {
        let value = args.into_iter().next();
        match (self.self_stack.last(), value) {
            (Some(ctx), Some(value)) if ctx.name.as_ref() == "array" => {
                Ty::Array(Interned::new(value))
            }
            (Some(ctx), Some(value)) if ctx.name.as_ref() == "dictionary" => {
                Ty::Dict(RecordTy::new(vec![("..".into(), value)]))
            }
            (Some(ctx), _) => ctx.ty.clone().unwrap_or(Ty::Any),
            _ => Ty::Any,
        }
    }

    fn term_param(&mut self, k: &str, mut args: Args, attrs: ParamAttrs) -> Option<Ty> {
        let ty = self.define(k, &args.eat::<Value>().ok()??);
        let default = if let Some(default) = args.named::<Value>("default").ok().flatten() {
            Some(default.repr())
        } else {
            args.eat::<Value>()
                .ok()
                .flatten()
                .map(|default| default.repr())
        };
        let required = args
            .named::<bool>("required")
            .ok()
            .flatten()
            .unwrap_or(attrs.positional && !attrs.variadic);

        Some(Ty::Param(Interned::new(ParamTy {
            name: k.into(),
            docs: None,
            default,
            required,
            ty,
            attrs,
        })))
    }

    fn define_value(&mut self, v: &Value) -> TyMark {
        TyMark::Norm(self.term_value(v))
    }

    /// Gets the type of a value.
    fn term_value(&mut self, value: &Value) -> Ty {
        match value {
            Value::Array(a) => {
                let values = a
                    .iter()
                    .map(|v| self.term_value_rec(v, Span::detached()))
                    .collect::<Vec<_>>();
                Ty::Tuple(values.into())
            }
            // todo: term arguments
            Value::Args(..) => Ty::Builtin(BuiltinTy::Args),
            Value::Dict(dict) => {
                let values = dict
                    .iter()
                    .map(|(k, v)| (k.as_str().into(), self.term_value_rec(v, Span::detached())))
                    .collect();
                Ty::Dict(RecordTy::new(values))
            }
            Value::Module(module) => {
                let values = module
                    .scope()
                    .iter()
                    .map(|(k, b)| (k.into(), self.term_value_rec(b.read(), b.span())))
                    .collect();
                Ty::Dict(RecordTy::new(values))
            }
            Value::Type(ty) => Ty::Builtin(BuiltinTy::Type(*ty)),
            Value::Dyn(dyn_val) => Ty::Builtin(BuiltinTy::Type(dyn_val.ty())),
            Value::Func(func) => match func.inner() {
                Repr::Closure(closure) => self.term_sig(func, closure).unwrap_or(Ty::Any),
                Repr::With(..) | Repr::Native(..) | Repr::Element(..) | Repr::Plugin(..) => {
                    Ty::Func(func_signature(func.clone()).type_sig())
                }
            },
            _ if is_plain_value(value) => Ty::Value(InsTy::new(value.clone())),
            _ => Ty::Any,
        }
    }

    fn term_value_rec(&mut self, value: &Value, s: Span) -> Ty {
        match value {
            Value::Type(ty) => Ty::Builtin(BuiltinTy::Type(*ty)),
            Value::Dyn(v) => Ty::Builtin(BuiltinTy::Type(v.ty())),
            Value::None
            | Value::Auto
            | Value::Array(..)
            | Value::Args(..)
            | Value::Dict(..)
            | Value::Module(..)
            | Value::Func(..)
            | Value::Label(..)
            | Value::Bool(..)
            | Value::Int(..)
            | Value::Float(..)
            | Value::Decimal(..)
            | Value::Length(..)
            | Value::Angle(..)
            | Value::Ratio(..)
            | Value::Relative(..)
            | Value::Fraction(..)
            | Value::Color(..)
            | Value::Gradient(..)
            | Value::Tiling(..)
            | Value::Symbol(..)
            | Value::Version(..)
            | Value::Str(..)
            | Value::Bytes(..)
            | Value::Datetime(..)
            | Value::Duration(..)
            | Value::Content(..)
            | Value::Styles(..) => {
                if !s.is_detached() {
                    Ty::Value(InsTy::new_at(value.clone(), s))
                } else {
                    Ty::Value(InsTy::new(value.clone()))
                }
            }
        }
    }

    fn term_sig(&mut self, func: &Func, closure: &Closure) -> Option<Ty> {
        let ret_value = func
            .call::<Vec<Value>>(&mut self.engine, Context::default().track(), vec![])
            .ok()?;

        let mut pos = vec![];
        let mut named = vec![];
        let mut rest_left = None;
        let mut rest_right = None;

        let syntax = match &closure.node {
            ClosureNode::Closure(node) => node.cast::<ast::Closure>()?,
            ClosureNode::Context(_) => return None,
        };
        let name = syntax.name().unwrap_or_default().get();
        let mut defaults = closure.defaults.iter();

        for param in syntax.params().children() {
            match param {
                ast::Param::Pos(p) => {
                    pos.push(Ty::Any);
                }
                ast::Param::Spread(r) => {
                    if pos.is_empty() {
                        rest_left = Some(Ty::Any);
                    } else {
                        rest_right = Some(Ty::Any);
                    }
                }
                ast::Param::Named(n) => {
                    let default = defaults.next();
                    let ty = if let Some(default) = default {
                        self.define(n.name().get(), default)
                    } else {
                        Ty::Any
                    };

                    match ty {
                        Ty::Param(p) => {
                            if p.attrs.positional {
                                pos.push(p.ty.clone());
                            } else if p.attrs.named {
                                named.push((p.name.clone(), p.ty.clone()));
                            } else if p.attrs.variadic {
                                if pos.is_empty() {
                                    rest_left = Some(p.ty.clone());
                                } else {
                                    rest_right = Some(p.ty.clone());
                                }
                            }
                        }
                        _ => {
                            named.push((n.name().get().into(), ty));
                        }
                    }
                }
            }
        }

        if rest_left.is_some() && rest_right.is_none() && pos.is_empty() {
            // If we have a rest left but no positional parameters, we treat it as a
            // rest right.
            rest_right = rest_left.take();
        }

        let ret = self.define_with_mark("ret", &ret_value);

        let ret = match ret {
            TyMark::Any => None,
            TyMark::Norm(ty) => Some(ty),
            TyMark::Sig(ty) => return Some(ty),
        };

        Some(Ty::Func(
            SigTy::new(pos.into_iter(), named, rest_left, rest_right, ret).into(),
        ))
    }
}

#[cfg(test)]
pub mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet},
        path::Path,
    };

    use super::*;

    use tinymist_world::{ShadowApi, args::CompileOnceArgs};
    use typst::{
        Feature, Features, Library, LibraryExt, World,
        engine::{Route, Sink, Traced},
        foundations::{Bytes, Scope, Type},
        introspection::Introspector,
    };

    use crate::{tests::*, ty::TypeInfo, ty::def::TypeInterface};

    macro_rules! typ_path {
        ($path:expr) => {
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../typ/", $path)
        };
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
    enum ParamShapeKind {
        Pos,
        Named,
        PosNamed,
        Rest,
        Other,
    }

    #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    struct ParamShape {
        name: String,
        kind: ParamShapeKind,
        required: bool,
        default: Option<String>,
    }

    type MethodShapes = BTreeMap<String, Vec<ParamShape>>;
    type TypeShapes = BTreeMap<String, MethodShapes>;

    fn param_shape_kind(attrs: ParamAttrs) -> ParamShapeKind {
        match (attrs.positional, attrs.named, attrs.variadic) {
            (_, _, true) => ParamShapeKind::Rest,
            (true, true, false) => ParamShapeKind::PosNamed,
            (true, false, false) => ParamShapeKind::Pos,
            (false, true, false) => ParamShapeKind::Named,
            (false, false, false) => ParamShapeKind::Other,
        }
    }

    fn with_test_worker<T>(
        world: &dyn World,
        scheme: &mut TypeInfo,
        f: impl FnOnce(&mut TySchemeWorker<'_>) -> T,
    ) -> T {
        let route = Route::default();
        let mut sink = Sink::default();
        let introspector = Introspector::default();
        let traced = Traced::default();
        let engine = Engine {
            routines: &typst::ROUTINES,
            world: world.track(),
            introspector: introspector.track(),
            traced: traced.track(),
            sink: sink.track_mut(),
            route,
        };

        let mut worker = TySchemeWorker {
            engine,
            scheme,
            self_stack: vec![],
            tv_stack: vec![],
            fresh_vars: 0,
        };
        f(&mut worker)
    }

    fn map_typings_std(world: &mut tinymist_project::LspWorld) {
        let main_id = world.main();
        world
            .map_shadow_by_id(
                main_id.join("/typings.typ"),
                Bytes::from_string(include_str!(typ_path!("packages/typings/lib.typ"))),
            )
            .unwrap();
        world
            .map_shadow_by_id(
                main_id.join("/std.typ"),
                Bytes::from_string(
                    include_str!(typ_path!("typings/std.typ"))
                        .replace("/typ/packages/typings/lib.typ", "typings.typ"),
                ),
            )
            .unwrap();
    }

    fn parse_typings_std(
        world: &tinymist_project::LspWorld,
    ) -> (typst::foundations::Module, typst::syntax::Source) {
        let std_id = world.main().join("/std.typ");
        let source = world.source(std_id).unwrap();
        let module = typst_shim::eval::eval_compat(world, &source).unwrap_or_else(|err| {
            panic!("Failed to evaluate typings std module: {err:?}");
        });

        (module, source)
    }

    fn collect_upstream_type_shapes() -> TypeShapes {
        let library = Library::builder()
            .with_features(Features::from_iter([Feature::Html]))
            .build();
        let mut result = TypeShapes::new();
        let mut seen_scopes = BTreeSet::new();
        let mut scopes = vec![
            ("std".to_string(), library.global.scope()),
            ("math".to_string(), library.math.scope()),
        ];

        while let Some((path, scope)) = scopes.pop() {
            let scope_id = scope as *const Scope as usize;
            if !seen_scopes.insert(scope_id) {
                continue;
            }

            for (name, binding) in scope.iter() {
                let path = format!("{path}.{name}");
                match binding.read() {
                    Value::Func(func) => {
                        if let Some(scope) = func.scope() {
                            scopes.push((path, scope));
                        }
                    }
                    Value::Module(module) => {
                        scopes.push((path, module.scope()));
                    }
                    Value::Type(ty) => {
                        let export = path.rsplit('.').next().unwrap().to_string();
                        let methods = upstream_type_method_shapes(*ty);
                        if let Some(prev) = result.insert(export.clone(), methods.clone()) {
                            assert_eq!(prev, methods, "duplicate std type export {export}");
                        }
                        scopes.push((path, ty.scope()));
                    }
                    _ => {}
                }
            }
        }

        result
    }

    fn upstream_type_method_shapes(ty: Type) -> MethodShapes {
        let mut methods = MethodShapes::new();
        for (name, binding) in ty.scope().iter() {
            if let Value::Func(func) = binding.read() {
                let params = func
                    .params()
                    .map(|params| {
                        params
                            .iter()
                            .map(|param| ParamShape {
                                name: param.name.to_string(),
                                kind: param_shape_kind(ParamAttrs::from(param)),
                                required: param.required && !param.variadic,
                                default: param.default.map(|default| {
                                    crate::upstream::truncated_repr(&default()).to_string()
                                }),
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                methods.insert(name.to_string(), params);
            }
        }

        methods
    }

    fn typing_value_args(value: &Value) -> Option<Args> {
        let Value::Func(func) = value else {
            return None;
        };
        let Repr::With(with) = func.inner() else {
            return None;
        };

        Some(with.1.clone())
    }

    fn generated_type_method_shapes(
        worker: &mut TySchemeWorker,
        export: &str,
        value: &Value,
    ) -> MethodShapes {
        let mut args = typing_value_args(value)
            .unwrap_or_else(|| panic!("{export} is not a typing item with arguments"));
        let kind = args
            .named::<Str>("kind")
            .ok()
            .flatten()
            .unwrap_or_else(|| panic!("{export} has no typing kind"));
        assert_eq!(kind.as_str(), "rec", "{export} must be generated with rec");

        let scope = args
            .named::<Value>("scope")
            .ok()
            .flatten()
            .unwrap_or_else(|| panic!("{export} has no generated scope"));
        let Value::Dict(scope) = scope else {
            panic!("{export} generated scope is not a dictionary");
        };

        scope
            .iter()
            .filter_map(|(method, value)| {
                let Value::Func(func) = value else {
                    return None;
                };
                Some((
                    method.to_string(),
                    generated_method_param_shapes(worker, export, method, func),
                ))
            })
            .collect()
    }

    fn generated_method_param_shapes(
        worker: &mut TySchemeWorker,
        export: &str,
        method: &str,
        func: &Func,
    ) -> Vec<ParamShape> {
        let Repr::Closure(closure) = func.inner() else {
            panic!("{export}.{method} is not generated as a closure");
        };
        let syntax = match &closure.node {
            ClosureNode::Closure(node) => node.cast::<ast::Closure>().unwrap(),
            ClosureNode::Context(_) => panic!("{export}.{method} is a context closure"),
        };
        let mut defaults = closure.defaults.iter();
        let mut params = vec![];

        for param in syntax.params().children() {
            match param {
                ast::Param::Pos(pos) => {
                    let name = pos
                        .bindings()
                        .into_iter()
                        .next()
                        .map(|ident| ident.get().to_string())
                        .unwrap_or_default();
                    params.push(ParamShape {
                        name,
                        kind: ParamShapeKind::Pos,
                        required: true,
                        default: None,
                    });
                }
                ast::Param::Spread(spread) => {
                    params.push(ParamShape {
                        name: spread
                            .sink_ident()
                            .map(|sink| sink.get().to_string())
                            .unwrap_or_default(),
                        kind: ParamShapeKind::Rest,
                        required: false,
                        default: None,
                    });
                }
                ast::Param::Named(named) => {
                    let name = named.name().get();
                    let default = defaults.next();
                    let ty = if let Some(default) = default {
                        worker.define(name, default)
                    } else {
                        Ty::Any
                    };

                    match ty {
                        Ty::Param(param) => {
                            if param.attrs.variadic {
                                assert!(
                                    matches!(param.ty, Ty::Array(_) | Ty::Tuple(_)),
                                    "{export}.{method}.{} rest type must be an array or tuple: {:?}",
                                    param.name,
                                    param.ty
                                );
                            }
                            params.push(ParamShape {
                                name: param.name.to_string(),
                                kind: param_shape_kind(param.attrs),
                                required: param.required,
                                default: param.default.as_ref().map(|default| default.to_string()),
                            });
                        }
                        _ => panic!(
                            "{export}.{method}.{name} must use pos/named/pos-named/rest typing metadata"
                        ),
                    }
                }
            }
        }

        params
    }

    fn assert_parser_keeps_method_names(export: &str, ty: Ty, expected: &MethodShapes) {
        let Ty::Dict(record) = ty else {
            panic!("{export} did not parse to a record type: {ty:?}");
        };

        let actual = record
            .interface()
            .filter_map(|(name, ty)| matches!(ty, Ty::Func(_)).then(|| name.to_string()))
            .collect::<BTreeSet<_>>();
        let expected = expected.keys().cloned().collect::<BTreeSet<_>>();

        assert_eq!(
            actual, expected,
            "{export} method names differ after parsing"
        );
    }

    fn array_map_self_var(ty: &Ty) -> Interned<TypeVar> {
        let Ty::Dict(record) = ty else {
            panic!("array did not parse to a record type: {ty:?}");
        };
        let map = record
            .field_by_name(&Interned::new_str("map"))
            .unwrap_or_else(|| panic!("array has no map method"));
        let Ty::Func(sig) = map else {
            panic!("array.map did not parse to a function: {map:?}");
        };
        let Ty::Array(elem) = sig
            .pos(0)
            .unwrap_or_else(|| panic!("array.map has no self"))
        else {
            panic!("array.map self is not an array: {sig:?}");
        };
        let Ty::Var(var) = elem.as_ref() else {
            panic!("array.map self element is not a type variable: {elem:?}");
        };

        var.clone()
    }

    #[test]
    fn test_check() {
        snapshot_testing("type_schema", &|mut world, path| {
            map_typings_std(&mut world);
            let main_id = world.main();
            let source = world.source(main_id).unwrap();

            let module = typst_shim::eval::eval_compat(&world, &source).unwrap_or_else(|err| {
                panic!("Failed to evaluate module ({path:?}): {err:?}");
            });

            let mut scheme = TypeInfo::default();
            with_test_worker(&world, &mut scheme, |w| {
                for (k, v) in module.scope().iter() {
                    let fid = v.span().id().unwrap();
                    if fid != source.id() {
                        continue;
                    }

                    let ty = w.define(k, v.read());
                    w.scheme.exports.insert(k.into(), ty);
                }
            });

            let result = format!("{:#?}", TypeCheckSnapshot(&scheme));

            insta::assert_snapshot!(result);
        });
    }

    #[test]
    fn std_typings_match_upstream_shapes() {
        let mut expected = collect_upstream_type_shapes();
        expected.remove("arguments");

        tinymist_tests::run_with_sources("#import \"std.typ\": *", |verse, _| {
            let mut world = verse.snapshot();
            map_typings_std(&mut world);
            let (module, source) = parse_typings_std(&world);

            let mut actual = TypeShapes::new();
            let mut scheme = TypeInfo::default();
            with_test_worker(&world, &mut scheme, |worker| {
                for (export, expected_methods) in &expected {
                    let binding = module
                        .scope()
                        .get(export)
                        .unwrap_or_else(|| panic!("missing generated std type {export}"));
                    assert_eq!(
                        binding.span().id(),
                        Some(source.id()),
                        "{export} must be defined by typ/typings/std.typ"
                    );

                    let parsed = worker.define(export, binding.read());
                    assert_parser_keeps_method_names(export, parsed, expected_methods);
                    actual.insert(
                        export.clone(),
                        generated_type_method_shapes(worker, export, binding.read()),
                    );
                }
            });

            assert_eq!(actual, expected);
        });
    }

    #[test]
    fn std_array_type_vars_are_fresh_per_expansion() {
        tinymist_tests::run_with_sources("#import \"std.typ\": *", |verse, _| {
            let mut world = verse.snapshot();
            map_typings_std(&mut world);
            let (module, _) = parse_typings_std(&world);

            let mut scheme = TypeInfo::default();
            with_test_worker(&world, &mut scheme, |worker| {
                let binding = module
                    .scope()
                    .get("array")
                    .unwrap_or_else(|| panic!("missing generated std type array"));
                let first = worker.define("array", binding.read());
                let second = worker.define("array", binding.read());

                assert_ne!(array_map_self_var(&first), array_map_self_var(&second));
            });
        });
    }
}
