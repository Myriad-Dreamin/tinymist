//! Completion kind analysis.

use super::*;

pub(crate) struct CompletionKindChecker {
    pub symbols: HashSet<char>,
    pub functions: HashSet<Ty>,
}

impl CompletionKindChecker {
    /// Reset the checker status for a fresh checking.
    fn reset(&mut self) {
        self.symbols.clear();
        self.functions.clear();
    }

    /// Check the completion kind of a type.
    pub fn check(&mut self, ty: &Ty) {
        self.reset();
        match ty {
            Ty::Value(val) => match &val.val {
                Value::Type(t) if t.constructor().is_ok() => {
                    self.functions.insert(ty.clone());
                }
                Value::Func(..) => {
                    self.functions.insert(ty.clone());
                }
                Value::Symbol(s) => {
                    self.symbols.insert(s.get());
                }
                _ => {}
            },
            Ty::Func(..) | Ty::With(..) => {
                self.functions.insert(ty.clone());
            }
            Ty::Builtin(BuiltinTy::TypeType(t)) if t.constructor().is_ok() => {
                self.functions.insert(ty.clone());
            }
            Ty::Builtin(BuiltinTy::Element(..)) => {
                self.functions.insert(ty.clone());
            }
            Ty::Let(bounds) => {
                for bound in bounds.ubs.iter().chain(bounds.lbs.iter()) {
                    self.check(bound);
                }
            }
            Ty::Any
            | Ty::Builtin(..)
            | Ty::Boolean(..)
            | Ty::Param(..)
            | Ty::Union(..)
            | Ty::Var(..)
            | Ty::Dict(..)
            | Ty::Array(..)
            | Ty::Tuple(..)
            | Ty::Args(..)
            | Ty::Pattern(..)
            | Ty::Select(..)
            | Ty::Unary(..)
            | Ty::Binary(..)
            | Ty::If(..) => {}
        }
    }
}

#[derive(Default, Debug)]
pub(crate) struct FnCompletionFeat {
    min_pos: Option<usize>,
    min_named: Option<usize>,
    pub has_rest: bool,
    pub next_arg_is_content: bool,
    pub is_element: bool,
}

impl FnCompletionFeat {
    pub fn check<'a>(mut self, fns: impl ExactSizeIterator<Item = &'a Ty>) -> Self {
        for ty in fns {
            self.check_one(ty, 0);
        }

        self
    }

    pub fn min_pos(&self) -> usize {
        self.min_pos.unwrap_or_default()
    }

    pub fn min_named(&self) -> usize {
        self.min_named.unwrap_or_default()
    }

    fn check_one(&mut self, ty: &Ty, pos: usize) {
        match ty {
            Ty::Value(val) => match &val.val {
                Value::Type(ty) => {
                    self.check_one(&Ty::Builtin(BuiltinTy::Type(*ty)), pos);
                }
                Value::Func(func) => {
                    if func.element().is_some() {
                        self.is_element = true;
                    }
                    let sig = func_signature(func.clone()).type_sig();
                    self.check_sig(&sig, pos);
                }
                Value::None
                | Value::Auto
                | Value::Bool(_)
                | Value::Int(_)
                | Value::Float(..)
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
                | Value::Label(..)
                | Value::Datetime(..)
                | Value::Decimal(..)
                | Value::Duration(..)
                | Value::Content(..)
                | Value::Styles(..)
                | Value::Array(..)
                | Value::Dict(..)
                | Value::Args(..)
                | Value::Module(..)
                | Value::Plugin(..)
                | Value::Dyn(..) => {}
            },
            Ty::Func(sig) => self.check_sig(sig, pos),
            Ty::With(w) => {
                self.check_one(&w.sig, pos + w.with.positional_params().len());
            }
            Ty::Builtin(b) => match b {
                BuiltinTy::Element(func) => {
                    self.is_element = true;
                    let func = (*func).into();
                    let sig = func_signature(func).type_sig();
                    self.check_sig(&sig, pos);
                }
                BuiltinTy::Type(ty) => {
                    let func = ty.constructor().ok();
                    if let Some(func) = func {
                        let sig = func_signature(func).type_sig();
                        self.check_sig(&sig, pos);
                    }
                }
                BuiltinTy::TypeType(..) => {}
                BuiltinTy::Clause
                | BuiltinTy::Undef
                | BuiltinTy::Content
                | BuiltinTy::Space
                | BuiltinTy::None
                | BuiltinTy::Break
                | BuiltinTy::Continue
                | BuiltinTy::Infer
                | BuiltinTy::FlowNone
                | BuiltinTy::Auto
                | BuiltinTy::Args
                | BuiltinTy::Color
                | BuiltinTy::TextSize
                | BuiltinTy::TextFont
                | BuiltinTy::TextLang
                | BuiltinTy::TextRegion
                | BuiltinTy::Label
                | BuiltinTy::CiteLabel
                | BuiltinTy::RefLabel
                | BuiltinTy::Dir
                | BuiltinTy::Length
                | BuiltinTy::Float
                | BuiltinTy::Stroke
                | BuiltinTy::Margin
                | BuiltinTy::Inset
                | BuiltinTy::Outset
                | BuiltinTy::Radius
                | BuiltinTy::Tag(..)
                | BuiltinTy::Module(..)
                | BuiltinTy::Path(..) => {}
            },
            Ty::Any
            | Ty::Boolean(..)
            | Ty::Param(..)
            | Ty::Union(..)
            | Ty::Let(..)
            | Ty::Var(..)
            | Ty::Dict(..)
            | Ty::Array(..)
            | Ty::Tuple(..)
            | Ty::Args(..)
            | Ty::Pattern(..)
            | Ty::Select(..)
            | Ty::Unary(..)
            | Ty::Binary(..)
            | Ty::If(..) => {}
        }
    }

    // todo: sig is element
    fn check_sig(&mut self, sig: &SigTy, idx: usize) {
        let pos_size = sig.positional_params().len();
        self.has_rest = self.has_rest || sig.rest_param().is_some();
        self.next_arg_is_content =
            self.next_arg_is_content || sig.pos(idx).is_some_and(|ty| ty.is_content(&()));
        let name_size = sig.named_params().len();
        let left_pos = pos_size.saturating_sub(idx);
        self.min_pos = self
            .min_pos
            .map_or(Some(left_pos), |v| Some(v.min(left_pos)));
        self.min_named = self
            .min_named
            .map_or(Some(name_size), |v| Some(v.min(name_size)));
    }
}

pub(crate) fn type_to_completion_kind(ty: &Ty) -> CompletionKind {
    match ty {
        Ty::Value(ins_ty) => value_to_completion_kind(&ins_ty.val),
        Ty::Func(..) | Ty::With(..) => CompletionKind::Func,
        Ty::Any => CompletionKind::Variable,
        Ty::Builtin(b) => match b {
            BuiltinTy::Module(..) => CompletionKind::Module,
            BuiltinTy::Type(..) | BuiltinTy::TypeType(..) => CompletionKind::Type,
            _ => CompletionKind::Variable,
        },
        Ty::Let(bounds) => fold_ty_kind(bounds.ubs.iter().chain(bounds.lbs.iter())),
        Ty::Union(types) => fold_ty_kind(types.iter()),
        Ty::Boolean(..)
        | Ty::Param(..)
        | Ty::Var(..)
        | Ty::Dict(..)
        | Ty::Array(..)
        | Ty::Tuple(..)
        | Ty::Args(..)
        | Ty::Pattern(..)
        | Ty::Select(..)
        | Ty::Unary(..)
        | Ty::Binary(..)
        | Ty::If(..) => CompletionKind::Constant,
    }
}

fn fold_ty_kind<'a>(tys: impl Iterator<Item = &'a Ty>) -> CompletionKind {
    tys.fold(None, |acc, ty| match acc {
        Some(CompletionKind::Variable) => Some(CompletionKind::Variable),
        Some(acc) => {
            let kind = type_to_completion_kind(ty);
            if acc == kind {
                Some(acc)
            } else {
                Some(CompletionKind::Variable)
            }
        }
        None => Some(type_to_completion_kind(ty)),
    })
    .unwrap_or(CompletionKind::Variable)
}

pub(crate) fn value_to_completion_kind(value: &Value) -> CompletionKind {
    match value {
        Value::Func(..) => CompletionKind::Func,
        Value::Plugin(..) | Value::Module(..) => CompletionKind::Module,
        Value::Type(..) => CompletionKind::Type,
        Value::Symbol(s) => CompletionKind::Symbol(s.get()),
        Value::None
        | Value::Auto
        | Value::Bool(..)
        | Value::Int(..)
        | Value::Float(..)
        | Value::Length(..)
        | Value::Angle(..)
        | Value::Ratio(..)
        | Value::Relative(..)
        | Value::Fraction(..)
        | Value::Color(..)
        | Value::Gradient(..)
        | Value::Pattern(..)
        | Value::Version(..)
        | Value::Str(..)
        | Value::Bytes(..)
        | Value::Label(..)
        | Value::Datetime(..)
        | Value::Decimal(..)
        | Value::Duration(..)
        | Value::Content(..)
        | Value::Styles(..)
        | Value::Array(..)
        | Value::Dict(..)
        | Value::Args(..)
        | Value::Dyn(..) => CompletionKind::Variable,
    }
}
