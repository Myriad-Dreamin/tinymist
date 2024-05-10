use core::fmt;
use ecow::{EcoString, EcoVec};
use once_cell::sync::OnceCell;
use reflexo::vector::ir::DefId;
use std::hash::{Hash, Hasher};
use typst::{
    foundations::Value,
    syntax::{ast, Span, SyntaxNode},
};

use crate::{
    adt::interner::{impl_internable, Interned},
    analysis::{BuiltinTy, UnaryOp},
};

pub type TyRef = Interned<Ty>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeSource {
    pub name_node: SyntaxNode,
    pub name_repr: OnceCell<Interned<str>>,
    pub span: Span,
    pub doc: Interned<str>,
}

impl Hash for TypeSource {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name_node.hash(state);
        self.span.hash(state);
        self.doc.hash(state);
    }
}

impl TypeSource {
    pub fn name(&self) -> Interned<str> {
        self.name_repr
            .get_or_init(|| {
                let name = self.name_node.text();
                if !name.is_empty() {
                    return Interned::new_str(name.as_str());
                }
                let name = self.name_node.clone().into_text();
                Interned::new_str(name.as_str())
            })
            .clone()
    }
}

#[derive(Debug, Hash, Clone, PartialEq)]
pub struct InsTy {
    pub val: Value,

    pub syntax: Option<Interned<TypeSource>>,
}

// There are some case that val is not Eq, but we make it Eq for simplicity
impl Eq for InsTy {}

impl InsTy {
    pub fn new(val: Value) -> Interned<Self> {
        Interned::new(Self { val, syntax: None })
    }
    pub fn new_at(val: Value, s: Span) -> Interned<Self> {
        Interned::new(Self {
            val,
            syntax: Some(Interned::new(TypeSource {
                name_node: SyntaxNode::default(),
                name_repr: OnceCell::new(),
                span: s,
                doc: Interned::new_str(""),
            })),
        })
    }
    pub fn new_doc(val: Value, doc: &str) -> Interned<Self> {
        Interned::new(Self {
            val,
            syntax: Some(Interned::new(TypeSource {
                name_node: SyntaxNode::default(),
                name_repr: OnceCell::new(),
                span: Span::detached(),
                doc: Interned::new_str(doc),
            })),
        })
    }
}

#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct NameBone {
    pub names: Vec<Interned<str>>,
}

impl NameBone {
    pub fn empty() -> Interned<Self> {
        Interned::new(Self { names: Vec::new() })
    }
}

impl NameBone {
    fn find(&self, name: &Interned<str>) -> Option<usize> {
        // binary search
        self.names.binary_search_by(|probe| probe.cmp(name)).ok()
    }
}

#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct FieldTy {
    pub name: Interned<str>,
    pub field: Ty,

    pub syntax: Option<Interned<TypeSource>>,
}
impl FieldTy {
    pub(crate) fn new_untyped(name: Interned<str>) -> Interned<Self> {
        Interned::new(Self {
            name,
            field: Ty::Any,
            syntax: None,
        })
    }
}

#[derive(Debug, Hash, Clone, PartialEq, Eq, Default)]
pub struct TypeBounds {
    pub lbs: EcoVec<Ty>,
    pub ubs: EcoVec<Ty>,
}

#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct TypeVar {
    pub name: Interned<str>,
    pub def: DefId,

    pub syntax: Option<Interned<TypeSource>>,
}

impl TypeVar {
    pub fn new(name: Interned<str>, def: DefId) -> Interned<Self> {
        Interned::new(Self {
            name,
            def,
            syntax: None,
        })
    }

    pub fn name(&self) -> Interned<str> {
        self.name.clone()
    }

    pub fn id(&self) -> DefId {
        self.def
    }
}

#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct RecordTy {
    pub types: Interned<Vec<Ty>>,
    pub names: Interned<NameBone>,
    pub syntax: Option<Interned<TypeSource>>,
}

impl RecordTy {
    pub(crate) fn shape_fields(mut fields: Vec<(EcoString, Ty, Span)>) -> (NameBone, Vec<Ty>) {
        fields.sort_by(|a, b| a.0.cmp(&b.0));
        let names = NameBone {
            names: fields
                .iter()
                .map(|(name, _, _)| Interned::new_str(name.as_str()))
                .collect(),
        };
        let types = fields.into_iter().map(|(_, ty, _)| ty).collect::<Vec<_>>();

        (names, types)
    }

    pub(crate) fn new(fields: Vec<(EcoString, Ty, Span)>) -> Interned<Self> {
        let (names, types) = Self::shape_fields(fields);
        Interned::new(Self {
            types: Interned::new(types),
            names: Interned::new(names),
            syntax: None,
        })
    }
}

enum ParamTy<'a> {
    Pos(&'a Ty),
    Named(&'a Interned<str>, &'a Ty),
    Rest(&'a Ty),
}

impl fmt::Debug for ParamTy<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParamTy::Pos(ty) => write!(f, "{:?}", ty),
            ParamTy::Named(name, ty) => write!(f, "{:?}: {:?}", name, ty),
            ParamTy::Rest(ty) => write!(f, "...: {:?}", ty),
        }
    }
}

#[derive(Hash, Clone, PartialEq, Eq)]
pub struct SigTy {
    pub types: Interned<Vec<Ty>>,
    pub ret: Option<Ty>,
    pub names: Interned<NameBone>,
    pub name_started: u32,
    pub spread_left: bool,
    pub spread_right: bool,
    pub has_free_variables: bool,

    pub syntax: Option<Interned<TypeSource>>,
}

impl SigTy {
    /// Array constructor
    #[comemo::memoize]
    pub(crate) fn array_cons(elem: Ty, anyify: bool) -> Interned<SigTy> {
        let ret = if anyify {
            Ty::Any
        } else {
            Ty::Array(Interned::new(elem.clone()))
        };
        Interned::new(Self {
            types: Interned::new(vec![elem]),
            ret: Some(ret),
            names: NameBone::empty(),
            name_started: 0,
            spread_left: false,
            spread_right: true,
            has_free_variables: false,
            syntax: None,
        })
    }

    /// Dictionary constructor
    #[comemo::memoize]
    pub(crate) fn dict_cons(named: &Interned<RecordTy>, anyify: bool) -> Interned<SigTy> {
        let ret = if anyify {
            Ty::Any
        } else {
            Ty::Dict(named.clone())
        };

        Interned::new(Self {
            types: named.types.clone(),
            ret: Some(ret),
            names: named.names.clone(),
            name_started: 0,
            spread_left: false,
            spread_right: false,
            has_free_variables: false,
            syntax: None,
        })
    }

    pub(crate) fn new(
        pos: impl Iterator<Item = Ty>,
        named: impl Iterator<Item = (EcoString, Ty)>,
        rest: Option<Ty>,
        ret_ty: Option<Ty>,
    ) -> Self {
        let named = named
            .map(|(name, ty)| (name, ty, Span::detached()))
            .collect::<Vec<_>>();
        let (names, types) = RecordTy::shape_fields(named);
        let spread_right = rest.is_some();

        let name_started = if spread_right { 1 } else { 0 } + types.len();
        let types = pos
            .chain(types.into_iter())
            .chain(rest.into_iter())
            .collect::<Vec<_>>();

        let name_started = (types.len() - name_started) as u32;

        Self {
            types: Interned::new(types),
            ret: ret_ty,
            names: Interned::new(names),
            name_started,
            spread_left: false,
            spread_right,
            // todo: substitute with actual value
            has_free_variables: false,
            syntax: None,
        }
    }
}

impl Default for SigTy {
    fn default() -> Self {
        Self {
            types: Interned::new(Vec::new()),
            ret: None,
            names: NameBone::empty(),
            name_started: 0,
            spread_left: false,
            spread_right: false,
            has_free_variables: false,
            syntax: None,
        }
    }
}

impl SigTy {
    fn positional_params(&self) -> impl Iterator<Item = &Ty> {
        self.types.iter().take(self.name_started as usize)
    }

    fn named_params(&self) -> impl Iterator<Item = (&Interned<str>, &Ty)> {
        let named_names = self.names.names.iter();
        let named_types = self.types.iter().skip(self.name_started as usize);

        named_names.zip(named_types)
    }

    fn rest_param(&self) -> Option<&Ty> {
        if self.spread_right {
            self.types.last()
        } else {
            None
        }
    }

    fn named(&self, name: &Interned<str>) -> Option<&Ty> {
        let idx = self.names.find(name)?;
        self.types.get(idx + self.name_started as usize)
    }

    pub(crate) fn matches<'a, 'b>(
        &'a self,
        args: &'b SigTy,
        withs: Option<&Vec<Interned<crate::analysis::SigTy>>>,
    ) -> impl Iterator<Item = (&'a Ty, &'b Ty)> {
        self.positional_params().zip(args.positional_params())
    }
}

impl fmt::Debug for SigTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("(")?;
        let pos = self.positional_params().map(|ty| ParamTy::Pos(ty));
        let named = self
            .named_params()
            .map(|(name, ty)| ParamTy::Named(name, ty));
        let rest = self.rest_param().map(|ty| ParamTy::Rest(ty));
        interpersed(f, pos.chain(named).chain(rest))
    }
}

pub type ArgsTy = SigTy;

#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct SigWithTy {
    pub sig: TyRef,
    pub with: Interned<ArgsTy>,
}

#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct SelectTy {
    pub ty: Interned<Ty>,
    pub select: Interned<str>,
}

#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct TypeUnary {
    pub lhs: Interned<Ty>,
    pub op: UnaryOp,
}

#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct TypeBinary {
    pub operands: Interned<(Ty, Ty)>,
    pub op: ast::BinOp,
}

#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub(crate) struct IfTy {
    pub cond: Interned<Ty>,
    pub then: Interned<Ty>,
    pub else_: Interned<Ty>,
}

#[derive(Hash, Clone, PartialEq, Eq)]
pub(crate) enum Ty {
    Clause,
    Undef,
    Content,
    Any,
    Space,
    None,
    Infer,
    FlowNone,
    Auto,
    Boolean(Option<bool>),
    Builtin(BuiltinTy),
    Value(Interned<InsTy>),
    Field(Interned<FieldTy>),

    Var(Interned<TypeVar>),
    Union(Interned<Vec<Ty>>),
    Let(Interned<TypeBounds>),

    Func(Interned<SigTy>),
    With(Interned<SigWithTy>),
    Args(Interned<ArgsTy>),
    Dict(Interned<RecordTy>),
    Array(Interned<Ty>),
    // Note: may contains spread types
    Tuple(Interned<Vec<Ty>>),
    Select(Interned<SelectTy>),
    Unary(Interned<TypeUnary>),
    Binary(Interned<TypeBinary>),
    If(Interned<IfTy>),
}

impl fmt::Debug for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Clause => f.write_str("Clause"),
            Ty::Undef => f.write_str("Undef"),
            Ty::Content => f.write_str("Content"),
            Ty::Any => f.write_str("Any"),
            Ty::Space => f.write_str("Space"),
            Ty::None => f.write_str("None"),
            Ty::Infer => f.write_str("Infer"),
            Ty::FlowNone => f.write_str("FlowNone"),
            Ty::Auto => f.write_str("Auto"),
            Ty::Builtin(t) => write!(f, "{t:?}"),
            Ty::Args(a) => write!(f, "&({a:?})"),
            Ty::Func(s) => write!(f, "{s:?}"),
            Ty::Dict(r) => write!(f, "{r:?}"),
            Ty::Array(a) => write!(f, "Array<{a:?}>"),
            Ty::Tuple(t) => {
                f.write_str("(")?;
                for t in t.iter() {
                    write!(f, "{t:?}, ")?;
                }
                f.write_str(")")
            }
            Ty::With(w) => write!(f, "({:?}).with(..{:?})", w.sig, w.with),
            Ty::Select(a) => write!(f, "{a:?}"),
            Ty::Union(u) => {
                f.write_str("(")?;
                if let Some((first, u)) = u.split_first() {
                    write!(f, "{first:?}")?;
                    for u in u {
                        write!(f, " | {u:?}")?;
                    }
                }
                f.write_str(")")
            }
            Ty::Let(v) => write!(f, "({v:?})"),
            Ty::Field(ff) => write!(f, "{:?}: {:?}", ff.name, ff.field),
            Ty::Var(v) => write!(f, "@{}", v.name()),
            Ty::Unary(u) => write!(f, "{u:?}"),
            Ty::Binary(b) => write!(f, "{b:?}"),
            Ty::If(i) => write!(f, "{i:?}"),
            Ty::Value(v) => write!(f, "{v:?}", v = v.val),
            Ty::Boolean(b) => {
                if let Some(b) = b {
                    write!(f, "{b}")
                } else {
                    f.write_str("Boolean")
                }
            }
        }
    }
}

impl_internable!(Ty,);
impl_internable!(InsTy,);
impl_internable!(FieldTy,);
impl_internable!(TypeSource,);
impl_internable!(TypeVar,);
impl_internable!(SigWithTy,);
impl_internable!(SigTy,);
impl_internable!(RecordTy,);
impl_internable!(SelectTy,);
impl_internable!(TypeUnary,);
impl_internable!(TypeBinary,);
impl_internable!(IfTy,);
impl_internable!(Vec<Ty>,);
impl_internable!(TypeBounds,);
impl_internable!(NameBone,);
impl_internable!((Ty, Ty),);

fn interpersed<T: fmt::Debug>(
    f: &mut fmt::Formatter<'_>,
    iter: impl Iterator<Item = T>,
) -> fmt::Result {
    let mut first = true;
    for arg in iter {
        if first {
            first = false;
        } else {
            f.write_str(", ")?;
        }
        arg.fmt(f)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_ty() {
        use super::*;
        let ty = Ty::Clause;
        let ty_ref = TyRef::new(ty.clone());
        assert_eq!(ty_ref, TyRef::new(ty));
    }
}
