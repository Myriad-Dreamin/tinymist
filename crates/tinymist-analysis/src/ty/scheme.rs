#![allow(unused)]

use core::fmt;
use std::{path::Path, sync::OnceLock};

use comemo::Track;
use typst::{
    engine::Engine,
    foundations::{func::Repr, Args, Context, Func, Str, Value},
    syntax::Source,
};

use super::{term_value, Ty};
use crate::ty::{BuiltinTy, Interned, TypeInfo};

pub struct TySchemeWorker<'a> {
    scheme: &'a mut TypeInfo,
}

impl TySchemeWorker<'_> {
    fn is_typing_item(&self, v: &Value) -> bool {
        matches!(v, Value::Func(f) if f.span().id().is_some_and(|s| s.vpath().as_rooted_path() == Path::new("/typings.typ")))
    }
}

impl TySchemeWorker<'_> {
    pub fn define(&mut self, k: &str, v: &Value) -> Ty {
        if !self.is_typing_item(v) {
            self.define_value(v)
        } else {
            self.define_typing_value(k, v)
        }
    }

    fn define_typing_value(&mut self, k: &str, v: &Value) -> Ty {
        let closure = match v {
            Value::Func(f) => f,
            _ => return Ty::Any,
        };

        let with = match closure.inner() {
            Repr::With(c) => c,
            _ => return Ty::Any,
        };

        self.define_typing(k, &with.0, with.1.clone())
    }

    fn define_typing(&mut self, k: &str, func: &Func, mut args: Args) -> Ty {
        match func.inner() {
            Repr::Native(..) | Repr::Element(..) | Repr::Plugin(..) => Ty::Any,
            Repr::With(with) => {
                args.items = with.1.items.iter().cloned().chain(args.items).collect();
                self.define_typing(k, &with.0, args)
            }
            Repr::Closure(closure) => self.ty_cons(k, args).unwrap_or(Ty::Any),
        }
    }

    fn ty_cons(&mut self, k: &str, mut args: Args) -> Option<Ty> {
        crate::log_debug_ct!("ty_cons({k}) args: {args:?}");
        let kind = args.named::<Str>("kind").ok()??;
        Some(match kind.as_str() {
            "var" => self.define(k, &args.eat::<Value>().ok()??),
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
            _ => Ty::Any,
        })
    }

    fn define_value(&self, v: &Value) -> Ty {
        let ty = term_value(v);
        match ty {
            Ty::Builtin(BuiltinTy::TypeType(ty)) => Ty::Builtin(BuiltinTy::Type(ty)),
            ty => ty,
        }
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

    #[test]
    fn test_check() {
        snapshot_testing("type_schema", &|mut world, _path| {
            let main_id = world.main();
            world
                .map_shadow_by_id(
                    main_id.join("/typings.typ"),
                    Bytes::from_string(include_str!("typings.typ")),
                )
                .unwrap();
            world
                .map_shadow_by_id(
                    main_id.join("/builtin.typ"),
                    Bytes::from_string(include_str!("builtin.typ")),
                )
                .unwrap();
            let source = world.source(main_id).unwrap();

            let module = typst_shim::eval::eval_compat(&world, &source).unwrap();

            let mut scheme = TypeInfo::default();
            let mut w = TySchemeWorker {
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
