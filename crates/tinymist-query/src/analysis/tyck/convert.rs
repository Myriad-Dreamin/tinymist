use super::*;

pub fn term_value(ctx: &Arc<SharedContext>, value: &Value) -> Ty {
    match value {
        Value::Array(a) => {
            let values = a.iter().map(term_value_rec).collect::<Vec<_>>();
            Ty::Tuple(values.into())
        }
        // todo: term arguments
        Value::Args(..) => Ty::Builtin(BuiltinTy::Args),
        Value::Plugin(p) => {
            // todo: create infer variables for plugin functions
            let values = p
                .iter()
                .map(|k| (k.as_str().into(), Ty::Func(SigTy::any())))
                .collect();
            Ty::Dict(RecordTy::new(values))
        }
        Value::Dict(d) => {
            let values = d
                .iter()
                .map(|(k, v)| (k.as_str().into(), term_value_rec(v)))
                .collect();
            Ty::Dict(RecordTy::new(values))
        }
        Value::Module(m) => {
            let values = m
                .scope()
                .iter()
                .map(|(k, v, _)| (k.into(), term_value_rec(v)))
                .collect();
            Ty::Dict(RecordTy::new(values))
        }
        Value::Type(ty) => Ty::Builtin(BuiltinTy::Type(*ty)),
        Value::Dyn(v) => Ty::Builtin(BuiltinTy::Type(v.ty())),
        Value::Func(func) => Ty::Func(ctx.type_of_func(func.clone()).type_sig()),
        Value::Label(..)
        | Value::None
        | Value::Auto
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
        | Value::Pattern(..)
        | Value::Symbol(..)
        | Value::Version(..)
        | Value::Str(..)
        | Value::Bytes(..)
        | Value::Datetime(..)
        | Value::Duration(..)
        | Value::Content(..)
        | Value::Styles(..) => Ty::Value(InsTy::new(value.clone())),
    }
}

pub fn term_value_rec(value: &Value) -> Ty {
    match value {
        Value::Type(ty) => Ty::Builtin(BuiltinTy::Type(*ty)),
        Value::Dyn(v) => Ty::Builtin(BuiltinTy::Type(v.ty())),
        Value::None
        | Value::Auto
        | Value::Array(..)
        | Value::Args(..)
        | Value::Plugin(..)
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
        | Value::Pattern(..)
        | Value::Symbol(..)
        | Value::Version(..)
        | Value::Str(..)
        | Value::Bytes(..)
        | Value::Datetime(..)
        | Value::Duration(..)
        | Value::Content(..)
        | Value::Styles(..) => Ty::Value(InsTy::new(value.clone())),
    }
}
