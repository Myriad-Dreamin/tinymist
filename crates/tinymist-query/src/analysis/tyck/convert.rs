use crate::analysis::func_signature;

use super::*;

pub fn is_plain_value(value: &Value) -> bool {
    matches!(
        value,
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
            | Value::Styles(..)
    )
}

/// Gets the type of a value.
#[comemo::memoize]
pub fn term_value(value: &Value) -> Ty {
    match value {
        Value::Array(a) => {
            let values = a
                .iter()
                .map(|v| term_value_rec(v, Span::detached()))
                .collect::<Vec<_>>();
            Ty::Tuple(values.into())
        }
        // todo: term arguments
        Value::Args(args) => Ty::Lit(LitTy::Args(Some(args.clone()))),
        Value::Plugin(plugin) => {
            // todo: create infer variables for plugin functions
            let values = plugin
                .iter()
                .map(|method| (method.as_str().into(), Ty::Func(SigTy::any())))
                .collect();
            Ty::Dict(RecordTy::new(values))
        }
        Value::Dict(dict) => {
            let values = dict
                .iter()
                .map(|(k, v)| (k.as_str().into(), term_value_rec(v, Span::detached())))
                .collect();
            Ty::Dict(RecordTy::new(values))
        }
        Value::Module(module) => {
            let values = module
                .scope()
                .iter()
                .map(|(k, v, s)| (k.into(), term_value_rec(v, s)))
                .collect();
            Ty::Dict(RecordTy::new(values))
        }
        Value::Type(ty) => Ty::Lit(LitTy::TypeType(*ty)),
        Value::Dyn(dyn_val) => Ty::Lit(LitTy::Type(dyn_val.ty())),
        Value::Func(func) => Ty::Func(func_signature(func.clone()).type_sig()),
        Value::None => Ty::Lit(LitTy::None),
        Value::Auto => Ty::Lit(LitTy::Auto),
        Value::Bool(val) => Ty::Boolean(Some(*val)),
        Value::Int(val) => Ty::Lit(LitTy::Int(Some(*val))),
        Value::Float(val) => Ty::Lit(LitTy::Float(Some(*val))),
        Value::Decimal(val) => Ty::Lit(LitTy::Decimal(Some(*val))),
        Value::Length(val) => Ty::Lit(LitTy::Length(Some(*val))),
        Value::Angle(val) => Ty::Lit(LitTy::Angle(Some(*val))),
        Value::Ratio(val) => Ty::Lit(LitTy::Ratio(Some(*val))),
        Value::Relative(val) => Ty::Lit(LitTy::Relative(Some(*val))),
        Value::Fraction(val) => Ty::Lit(LitTy::Fraction(Some(*val))),
        Value::Color(val) => Ty::Lit(LitTy::Color(Some(*val))),
        Value::Gradient(val) => Ty::Lit(LitTy::Gradient(Some(val.clone()))),
        Value::Pattern(val) => Ty::Lit(LitTy::Pattern(Some(val.clone()))),
        Value::Symbol(val) => Ty::Lit(LitTy::Symbol(Some(val.clone()))),
        Value::Version(val) => Ty::Lit(LitTy::Version(Some(val.clone()))),
        Value::Str(val) => Ty::Lit(LitTy::Str(Some(val.clone()))),
        Value::Bytes(val) => Ty::Lit(LitTy::Bytes(Some(val.clone()))),
        Value::Datetime(val) => Ty::Lit(LitTy::Datetime(Some(*val))),
        Value::Duration(val) => Ty::Lit(LitTy::Duration(Some(*val))),
        Value::Content(val) => Ty::Lit(LitTy::Content(Some(val.clone()))),
        Value::Styles(val) => Ty::Lit(LitTy::Styles(Some(val.clone()))),
        Value::Label(val) => Ty::Lit(LitTy::Label(Some(*val))),
    }
}

pub fn term_value_rec(value: &Value, s: Span) -> Ty {
    match value {
        Value::Type(ty) => Ty::Lit(LitTy::TypeType(*ty)),
        Value::Dyn(v) => Ty::Lit(LitTy::Type(v.ty())),
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
        | Value::Styles(..) => {
            if !s.is_detached() {
                Ty::Value(InsTy::new_at(value.clone(), s))
            } else {
                Ty::Value(InsTy::new(value.clone()))
            }
        }
    }
}
