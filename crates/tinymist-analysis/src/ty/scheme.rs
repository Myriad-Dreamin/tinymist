#![allow(unused)]

use core::fmt;
use std::{path::Path, sync::OnceLock};

use comemo::Track;
use tinymist_world::args;
use typst::{
    engine::Engine,
    foundations::{func::Repr, Args, Closure, Context, Func, Str, Value},
    syntax::{ast, Source, Span},
};

use super::Ty;
use crate::{
    func_signature,
    syntax::Decl,
    ty::{
        is_plain_value, BuiltinTy, InsTy, Interned, ParamAttrs, ParamTy, RecordTy, SigTy, TypeInfo,
        TypeVar,
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
                Ty::Var(TypeVar::new(
                    name.as_str().into(),
                    Interned::new(Decl::lit_at(name.as_str(), args.span)),
                ))
            }
            "rec" => {
                // let ty = self.define(k, &args.eat::<Value>().ok()??);
                todo!()
            }
            "arr" => {
                let ty = self.define(k, &args.eat::<Value>().ok()??);
                Ty::Array(Interned::new(ty))
            }
            "tuple" => {
                let values = args.all::<Value>().ok()?;
                let values = values
                    .into_iter()
                    .map(|v| self.define(k, &v))
                    .collect::<Vec<_>>();
                Ty::Tuple(Interned::new(values))
            }
            "pos" => {
                let ty = self.define(k, &args.eat::<Value>().ok()??);
                Ty::Param(ParamTy::new(ty, k.into(), ParamAttrs::positional()))
            }
            "named" => {
                let ty = self.define(k, &args.eat::<Value>().ok()??);
                Ty::Param(ParamTy::new(ty, k.into(), ParamAttrs::named()))
            }
            "rest" => {
                let ty = self.define(k, &args.eat::<Value>().ok()??);
                Ty::Param(ParamTy::new(ty, k.into(), ParamAttrs::variadic()))
            }
            _ => Ty::Any,
        };
        Some(TyMark::Norm(ty))
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
        let ret = self.define_with_mark("ret", &ret_value);

        let ret = match ret {
            TyMark::Any => None,
            TyMark::Norm(ty) => Some(ty),
            TyMark::Sig(ty) => return Some(ty),
        };

        let syntax = closure.node.cast::<ast::Closure>()?;
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

        Some(Ty::Func(
            SigTy::new(pos.into_iter(), named, rest_left, rest_right, ret).into(),
        ))
    }
}

#[cfg(test)]
pub mod tests {
    use std::path::Path;

    use super::*;

    use tinymist_world::{args::CompileOnceArgs, ShadowApi};
    use typst::{
        engine::{Route, Sink, Traced},
        foundations::Bytes,
        introspection::Introspector,
        World,
    };

    use crate::{tests::*, ty::TypeInfo};

    macro_rules! typ_path {
        ($path:expr) => {
            concat!(env!("CARGO_MANIFEST_DIR"), "/../../typ/", $path)
        };
    }

    #[test]
    fn test_check() {
        snapshot_testing("type_schema", &|mut world, path| {
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
            let source = world.source(main_id).unwrap();

            let module = typst_shim::eval::eval_compat(&world, &source).unwrap_or_else(|err| {
                panic!("Failed to evaluate module ({path:?}): {err:?}");
            });

            let route = Route::default();
            let mut sink = Sink::default();
            let introspector = Introspector::default();
            let traced = Traced::default();
            let engine = Engine {
                routines: &typst::ROUTINES,
                world: ((&world) as &dyn World).track(),
                introspector: introspector.track(),
                traced: traced.track(),
                sink: sink.track_mut(),
                route,
            };

            let mut scheme = TypeInfo::default();
            let mut w = TySchemeWorker {
                engine,
                scheme: &mut scheme,
            };
            for (k, v) in module.scope().iter() {
                let fid = v.span().id().unwrap();
                if fid != source.id() {
                    continue;
                }

                let ty = w.define(k, v.read());
                w.scheme.exports.insert(k.into(), ty);
            }

            let result = format!("{:#?}", TypeCheckSnapshot(&scheme));

            insta::assert_snapshot!(result);
        });
    }
}
