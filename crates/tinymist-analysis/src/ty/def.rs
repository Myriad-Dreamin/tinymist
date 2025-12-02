//! Name Convention:
//! - `TypeXXX`: abstracted types or clauses
//! - `XXTy`: concrete types

use core::fmt;
use std::{
    hash::{Hash, Hasher},
    sync::{Arc, OnceLock},
};

use ecow::EcoString;
use parking_lot::{Mutex, RwLock};
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use typst::{
    foundations::{Content, Element, ParamInfo, Type, Value},
    syntax::{FileId, Span, SyntaxKind, SyntaxNode, ast},
};

use super::{BoundPred, BuiltinTy, PackageId};
use crate::{
    adt::{interner::impl_internable, snapshot_map},
    docs::UntypedDefDocs,
    syntax::{DeclExpr, UnaryOp},
};

pub(crate) use super::{TyCtx, TyCtxMut};
pub(crate) use crate::adt::interner::Interned;
pub use tinymist_derive::BindTyCtx;

/// A reference to the interned type.
pub(crate) type TyRef = Interned<Ty>;
/// A reference to the interned string.
pub(crate) type StrRef = Interned<str>;

/// All possible types in tinymist.
#[derive(Hash, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Ty {
    // Simple Types
    /// A top type, whose negation is bottom type.
    /// `t := top, t^- := bottom`
    Any,
    /// A boolean type, can be `false`, `true`, or both (boolean type).
    /// `t := false | true`
    Boolean(Option<bool>),
    /// All possible types in typst.
    Builtin(BuiltinTy),
    /// A possible typst instance of some type.
    Value(Interned<InsTy>),
    /// A parameter type
    Param(Interned<ParamTy>),

    // Combination Types
    /// A union type, whose negation is intersection type.
    /// `t := t1 | t2 | ... | tn, t^- := t1 & t2 & ... & tn`
    Union(Interned<Vec<Ty>>),
    /// A frozen type variable.
    /// `t :> t1 | t2 | ... | tn <: f1 & f2 & ... & fn`
    Let(Interned<TypeBounds>),
    /// An opening type variable owing bounds.
    Var(Interned<TypeVar>),

    // Composite Types
    /// A typst dictionary type.
    Dict(Interned<RecordTy>),
    /// An array type.
    Array(TyRef),
    /// A tuple type.
    /// Note: may contains spread types.
    Tuple(Interned<Vec<Ty>>),
    /// A function type.
    Func(Interned<SigTy>),
    /// An argument type.
    Args(Interned<ArgsTy>),
    /// A pattern type.
    Pattern(Interned<PatternTy>),

    // Type operations
    /// A partially applied function type.
    With(Interned<SigWithTy>),
    /// Select a field from a type.
    Select(Interned<SelectTy>),
    /// A unary operation.
    Unary(Interned<TypeUnary>),
    /// A binary operation.
    Binary(Interned<TypeBinary>),
    /// A conditional type.
    If(Interned<IfTy>),
}

impl fmt::Debug for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Any => f.write_str("Any"),
            Ty::Builtin(ty) => write!(f, "{ty:?}"),
            Ty::Args(args) => write!(f, "&({args:?})"),
            Ty::Func(func) => write!(f, "{func:?}"),
            Ty::Pattern(pat) => write!(f, "{pat:?}"),
            Ty::Dict(record) => write!(f, "{record:?}"),
            Ty::Array(arr) => write!(f, "Array<{arr:?}>"),
            Ty::Tuple(elems) => {
                f.write_str("(")?;
                for t in elems.iter() {
                    write!(f, "{t:?}, ")?;
                }
                f.write_str(")")
            }
            Ty::With(with) => write!(f, "({:?}).with(..{:?})", with.sig, with.with),
            Ty::Select(sel) => write!(f, "{sel:?}"),
            Ty::Union(types) => {
                f.write_str("(")?;
                if let Some((first, u)) = types.split_first() {
                    write!(f, "{first:?}")?;
                    for u in u {
                        write!(f, " | {u:?}")?;
                    }
                }
                f.write_str(")")
            }
            Ty::Let(bounds) => write!(f, "({bounds:?})"),
            Ty::Param(param) => write!(f, "{:?}: {:?}", param.name, param.ty),
            Ty::Var(var) => var.fmt(f),
            Ty::Unary(unary) => write!(f, "{unary:?}"),
            Ty::Binary(binary) => write!(f, "{binary:?}"),
            Ty::If(if_expr) => write!(f, "{if_expr:?}"),
            Ty::Value(ins_ty) => write!(f, "{:?}", ins_ty.val),
            Ty::Boolean(truthiness) => {
                if let Some(truthiness) = truthiness {
                    write!(f, "{truthiness}")
                } else {
                    f.write_str("Boolean")
                }
            }
        }
    }
}

impl Ty {
    /// Whether the type is a dictionary type.
    pub fn is_dict(&self) -> bool {
        matches!(self, Ty::Dict(..))
    }

    /// Creates a union type from two types.
    pub fn union(lhs: Option<Ty>, rhs: Option<Ty>) -> Option<Ty> {
        Some(match (lhs, rhs) {
            (Some(lhs), Some(rhs)) => Ty::from_types([lhs, rhs].into_iter()),
            (Some(ty), None) | (None, Some(ty)) => ty,
            (None, None) => return None,
        })
    }

    /// Creates a union type from an iterator of types.
    pub fn from_types(iter: impl ExactSizeIterator<Item = Ty>) -> Self {
        if iter.len() == 0 {
            Ty::Any
        } else if iter.len() == 1 {
            let mut iter = iter;
            iter.next().unwrap()
        } else {
            Self::iter_union(iter)
        }
    }

    /// Creates a union type from an iterator of types.     
    pub fn iter_union(iter: impl IntoIterator<Item = Ty>) -> Self {
        let mut v: Vec<Ty> = iter.into_iter().collect();
        v.sort();
        Ty::Union(Interned::new(v))
    }

    /// Creates an undefined type (which will emit an error).
    /// A that type is annotated if the syntax structure causes an type error.
    pub const fn undef() -> Self {
        Ty::Builtin(BuiltinTy::Undef)
    }

    /// Gets the name of the type.
    pub fn name(&self) -> Interned<str> {
        match self {
            Ty::Var(v) => v.name.clone(),
            Ty::Builtin(BuiltinTy::Module(m)) => m.name().clone(),
            ty => ty
                .value()
                .map(|_| Interned::new_str(&self.name()))
                .unwrap_or_default(),
        }
    }

    /// Gets the span of the type.
    pub fn span(&self) -> Span {
        fn seq(u: &[Ty]) -> Option<Span> {
            u.iter().find_map(|ty| {
                let sub = ty.span();
                if sub.is_detached() {
                    return None;
                }
                Some(sub)
            })
        }

        match self {
            Ty::Var(v) => v.def.span(),
            Ty::Let(u) => seq(&u.ubs)
                .or_else(|| seq(&u.lbs))
                .unwrap_or_else(Span::detached),
            Ty::Union(u) => seq(u).unwrap_or_else(Span::detached),
            _ => Span::detached(),
        }
    }

    /// Gets the value repr of the type.
    pub fn value(&self) -> Option<Value> {
        match self {
            Ty::Value(v) => Some(v.val.clone()),
            Ty::Builtin(BuiltinTy::Element(v)) => Some(Value::Func((*v).into())),
            Ty::Builtin(BuiltinTy::Type(ty)) => Some(Value::Type(*ty)),
            _ => None,
        }
    }

    /// Gets the element type.
    pub fn element(&self) -> Option<Element> {
        match self {
            Ty::Value(ins_ty) => match &ins_ty.val {
                Value::Func(func) => func.element(),
                _ => None,
            },
            Ty::Builtin(BuiltinTy::Element(v)) => Some(*v),
            _ => None,
        }
    }

    /// Checks a type against a context.
    pub fn satisfy<T: TyCtx>(&self, ctx: &T, f: impl FnMut(&Ty, bool)) {
        self.bounds(true, &mut BoundPred::new(ctx, f));
    }

    /// Checks if the type is a content type.
    pub fn is_content<T: TyCtx>(&self, ctx: &T) -> bool {
        let mut res = false;
        self.satisfy(ctx, |ty: &Ty, _pol| {
            res = res || {
                match ty {
                    Ty::Value(v) => is_content_builtin_type(&v.val.ty()),
                    Ty::Builtin(BuiltinTy::Content(..)) => true,
                    Ty::Builtin(BuiltinTy::Type(v)) => is_content_builtin_type(v),
                    _ => false,
                }
            }
        });
        res
    }

    /// Checks if the type is a string type.
    pub fn is_str<T: TyCtx>(&self, ctx: &T) -> bool {
        let mut res = false;
        self.satisfy(ctx, |ty: &Ty, _pol| {
            res = res || {
                match ty {
                    Ty::Value(v) => is_str_builtin_type(&v.val.ty()),
                    Ty::Builtin(BuiltinTy::Type(v)) => is_str_builtin_type(v),
                    _ => false,
                }
            }
        });
        res
    }

    /// Checks if the type is a type type.
    pub fn is_type<T: TyCtx>(&self, ctx: &T) -> bool {
        let mut res = false;
        self.satisfy(ctx, |ty: &Ty, _pol| {
            res = res || {
                match ty {
                    Ty::Value(v) => is_type_builtin_type(&v.val.ty()),
                    Ty::Builtin(BuiltinTy::Type(ty)) => is_type_builtin_type(ty),
                    Ty::Builtin(BuiltinTy::TypeType(..)) => true,
                    _ => false,
                }
            }
        });
        res
    }
}

/// Checks if the type is a content builtin type.
fn is_content_builtin_type(ty: &Type) -> bool {
    *ty == Type::of::<Content>() || *ty == Type::of::<typst::foundations::Symbol>()
}

/// Checks if the type is a string builtin type.
fn is_str_builtin_type(ty: &Type) -> bool {
    *ty == Type::of::<typst::foundations::Str>()
}

/// Checks if the type is a type builtin type.
fn is_type_builtin_type(ty: &Type) -> bool {
    *ty == Type::of::<Type>()
}

/// A function parameter type.
pub enum TypeSigParam<'a> {
    /// A positional parameter: `a`
    Pos(&'a Ty),
    /// A named parameter: `b: c`
    Named(&'a StrRef, &'a Ty),
    /// A rest parameter (spread right): `..d`
    Rest(&'a Ty),
}

impl fmt::Debug for TypeSigParam<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeSigParam::Pos(ty) => write!(f, "{ty:?}"),
            TypeSigParam::Named(name, ty) => write!(f, "{name:?}: {ty:?}"),
            // todo: the rest is not three dots
            TypeSigParam::Rest(ty) => write!(f, "...: {ty:?}"),
        }
    }
}

/// The syntax source (definition) of a type node.
/// todo: whether we should store them in the type node
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeSource {
    /// A name node with span.
    pub name_node: SyntaxNode,
    /// A lazy evaluated name.
    pub name_repr: OnceLock<StrRef>,
    /// The attached documentation.
    pub doc: StrRef,
}

impl Hash for TypeSource {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name_node.hash(state);
        self.doc.hash(state);
    }
}

impl TypeSource {
    /// Gets the name of the type node.
    pub fn name(&self) -> StrRef {
        self.name_repr
            .get_or_init(|| {
                let name = self.name_node.text();
                if !name.is_empty() {
                    return name.into();
                }
                let name = self.name_node.clone().into_text();
                name.into()
            })
            .clone()
    }
}

/// An ordered list of names.
#[derive(Debug, Hash, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct NameBone {
    /// The names in the bone.
    pub names: Box<[StrRef]>,
}

impl NameBone {
    /// Creates an empty bone.
    pub fn empty() -> Interned<Self> {
        Interned::new(Self {
            names: Box::new([]),
        })
    }
}

impl NameBone {
    /// Finds the index of the name in the bone.
    pub fn find(&self, name: &StrRef) -> Option<usize> {
        self.names.binary_search_by(|probe| probe.cmp(name)).ok()
    }
}

impl NameBone {
    /// Intersects the names of two bones.
    pub fn intersect_enumerate<'a>(
        &'a self,
        rhs: &'a NameBone,
    ) -> impl Iterator<Item = (usize, usize)> + 'a {
        let mut lhs_iter = self.names.iter().enumerate();
        let mut rhs_iter = rhs.names.iter().enumerate();

        let mut lhs = lhs_iter.next();
        let mut rhs = rhs_iter.next();

        std::iter::from_fn(move || {
            'name_scanning: loop {
                if let (Some((idx, lhs_key)), Some((j, rhs_key))) = (lhs, rhs) {
                    match lhs_key.cmp(rhs_key) {
                        std::cmp::Ordering::Less => {
                            lhs = lhs_iter.next();
                            continue 'name_scanning;
                        }
                        std::cmp::Ordering::Greater => {
                            rhs = rhs_iter.next();
                            continue 'name_scanning;
                        }
                        std::cmp::Ordering::Equal => {
                            lhs = lhs_iter.next();
                            rhs = rhs_iter.next();
                            return Some((idx, j));
                        }
                    }
                }
                return None;
            }
        })
    }
}

/// The state of a type variable (bounds of some type in program).
#[derive(Clone, Default)]
pub struct DynTypeBounds {
    /// The lower bounds
    pub lbs: rpds::HashTrieSetSync<Ty>,
    /// The upper bounds
    pub ubs: rpds::HashTrieSetSync<Ty>,
}

impl From<TypeBounds> for DynTypeBounds {
    fn from(bounds: TypeBounds) -> Self {
        Self {
            lbs: bounds.lbs.into_iter().collect(),
            ubs: bounds.ubs.into_iter().collect(),
        }
    }
}

impl DynTypeBounds {
    /// Gets the frozen bounds.
    pub fn freeze(&self) -> TypeBounds {
        // sorted
        let mut lbs: Vec<_> = self.lbs.iter().cloned().collect();
        lbs.sort();
        let mut ubs: Vec<_> = self.ubs.iter().cloned().collect();
        ubs.sort();
        TypeBounds { lbs, ubs }
    }
}

/// A frozen type variable (bounds of some type in program).
/// `t :> t1 | ... | tn <: f1 & ... & fn`
/// `  lbs------------- ubs-------------`
#[derive(Hash, Clone, PartialEq, Eq, Default, PartialOrd, Ord)]
pub struct TypeBounds {
    /// The lower bounds.
    pub lbs: Vec<Ty>,
    /// The upper bounds.
    pub ubs: Vec<Ty>,
}

impl fmt::Debug for TypeBounds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // write!(f, "{}", self.name)
        // also where
        if !self.lbs.is_empty() {
            write!(f, " ⪰ {:?}", self.lbs[0])?;
            for lb in &self.lbs[1..] {
                write!(f, " | {lb:?}")?;
            }
        }
        if !self.ubs.is_empty() {
            write!(f, " ⪯ {:?}", self.ubs[0])?;
            for ub in &self.ubs[1..] {
                write!(f, " & {ub:?}")?;
            }
        }
        Ok(())
    }
}

/// A common type kinds for those types that has fields (abstracted record
/// type).
pub trait TypeInterface {
    /// Gets the bone of a record.
    /// See [`NameBone`] for more details.
    fn bone(&self) -> &Interned<NameBone>;
    /// Iterates over the fields of a record.
    fn interface(&self) -> impl Iterator<Item = (&StrRef, &Ty)>;
    /// Gets the field by bone offset.
    fn field_by_bone_offset(&self, idx: usize) -> Option<&Ty>;
    /// Gets the field by name.
    fn field_by_name(&self, name: &StrRef) -> Option<&Ty> {
        self.field_by_bone_offset(self.bone().find(name)?)
    }
}

/// Extension common methods for [`TypeInterface`].
pub trait TypeInterfaceExt: TypeInterface {
    /// Convenience method to get the common fields of two records.
    fn common_iface_fields<'a>(
        &'a self,
        rhs: &'a Self,
    ) -> impl Iterator<Item = (&'a StrRef, &'a Ty, &'a Ty)> {
        let lhs_names = self.bone();
        let rhs_names = rhs.bone();

        lhs_names
            .intersect_enumerate(rhs_names)
            .filter_map(move |(i, j)| {
                let lhs = self.field_by_bone_offset(i)?;
                let rhs = rhs.field_by_bone_offset(j)?;
                Some((&lhs_names.names[i], lhs, rhs))
            })
    }
}

impl<T: TypeInterface> TypeInterfaceExt for T {}

/// An instance of a typst type.
#[derive(Debug, Hash, Clone, PartialEq)]
pub struct InsTy {
    /// The value of the instance.
    pub val: Value,
    /// The syntax source of the instance.
    pub syntax: Option<Interned<TypeSource>>,
}

/// There are some case that val is not Eq, but we make it Eq for simplicity
/// For example, a float instance which is NaN.
impl Eq for InsTy {}

impl PartialOrd for Interned<InsTy> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Interned<InsTy> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        cmp_value(&self.val, &other.val)
    }
}

fn cmp_value(x: &Value, y: &Value) -> std::cmp::Ordering {
    match x.partial_cmp(y) {
        Some(order) => return order,
        None => {
            let x_dis = val_discriminant(x);
            let y_dis = val_discriminant(y);
            if x_dis != y_dis {
                return x_dis.cmp(&y_dis);
            }
        }
    }

    match (&x, &y) {
        (Value::Str(x), Value::Str(y)) => x.cmp(y),
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        (Value::Int(x), Value::Int(y)) => x.cmp(y),
        (Value::Decimal(x), Value::Decimal(y)) => x.cmp(y),
        (Value::Angle(x), Value::Angle(y)) => x.cmp(y),
        (Value::Ratio(x), Value::Ratio(y)) => x.cmp(y),
        (Value::Fraction(x), Value::Fraction(y)) => x.cmp(y),
        (Value::Version(x), Value::Version(y)) => x.cmp(y),
        (Value::Bytes(x), Value::Bytes(y)) => x.cmp(y),
        (Value::Duration(x), Value::Duration(y)) => x.cmp(y),
        (Value::Type(x), Value::Type(y)) => x.cmp(y),
        (Value::None, Value::None) | (Value::Auto, Value::Auto) => std::cmp::Ordering::Equal,
        (Value::Array(x), Value::Array(y)) => cmp_by(x.iter(), y.iter(), cmp_value),
        (Value::Dict(x), Value::Dict(y)) => cmp_by(x.iter(), y.iter(), |(xk, xv), (yk, yv)| {
            xk.cmp(yk).then_with(|| cmp_value(xv, yv))
        }),
        (Value::Label(x), Value::Label(y)) => x.resolve().cmp(&y.resolve()),
        (Value::Float(x), Value::Float(y)) => x.to_bits().cmp(&y.to_bits()),
        (Value::Length(x), Value::Length(y)) => x.abs.cmp(&y.abs).then_with(|| x.em.cmp(&y.em)),
        (Value::Relative(x), Value::Relative(y)) => x.rel.cmp(&y.rel).then_with(|| {
            x.abs
                .abs
                .cmp(&y.abs.abs)
                .then_with(|| x.abs.em.cmp(&y.abs.em))
        }),
        (Value::Func(x), Value::Func(y)) => {
            if !x.span().is_detached() && !y.span().is_detached() {
                return x.span().into_raw().cmp(&y.span().into_raw());
            }

            use typst::foundations::func::Repr;
            match (x.inner(), y.inner()) {
                (Repr::Element(x), Repr::Element(y)) => x.cmp(y),
                _ => ptr_cmp(x, y),
            }
        }
        (Value::Args(x), Value::Args(y)) => {
            if !x.span.is_detached() && !y.span.is_detached() {
                return x.span.into_raw().cmp(&y.span.into_raw());
            }

            ptr_cmp(x, y)
        }
        (Value::Module(x), Value::Module(y)) => match (x.file_id(), y.file_id()) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(..), None) => std::cmp::Ordering::Less,
            (None, Some(..)) => std::cmp::Ordering::Greater,
            (None, None) => ptr_cmp(x, y),
        },
        (Value::Datetime(x), Value::Datetime(y)) => {
            x.partial_cmp(y).unwrap_or_else(|| ptr_cmp(x, y))
        }
        (Value::Color(x), Value::Color(y)) => ptr_cmp(x, y),
        (Value::Gradient(x), Value::Gradient(y)) => ptr_cmp(x, y),
        (Value::Tiling(x), Value::Tiling(y)) => ptr_cmp(x, y),
        (Value::Symbol(x), Value::Symbol(y)) => ptr_cmp(x, y),
        (Value::Content(x), Value::Content(y)) => ptr_cmp(x, y),
        (Value::Styles(x), Value::Styles(y)) => ptr_cmp(x, y),
        (Value::Dyn(x), Value::Dyn(y)) => ptr_cmp(x, y),
        _ => ptr_cmp(x, y),
    }
}

fn cmp_by<T>(
    mut x_iter: impl Iterator<Item = T>,
    mut y_iter: impl Iterator<Item = T>,
    mut cmp: impl FnMut(T, T) -> std::cmp::Ordering,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    loop {
        match (x_iter.next(), y_iter.next()) {
            (Some(x_item), Some(y_item)) => match cmp(x_item, y_item) {
                Ordering::Equal => continue,
                other => return other,
            },
            (Some(_), None) => return Ordering::Greater,
            (None, Some(_)) => return Ordering::Less,
            (None, None) => return Ordering::Equal,
        }
    }
}

fn val_discriminant(val: &Value) -> TypstValueEnum {
    match val {
        Value::Str(..) => TypstValueEnum::Str,
        Value::None => TypstValueEnum::None,
        Value::Auto => TypstValueEnum::Auto,
        Value::Array(..) => TypstValueEnum::Array,
        Value::Args(..) => TypstValueEnum::Args,
        Value::Dict(..) => TypstValueEnum::Dict,
        Value::Module(..) => TypstValueEnum::Module,
        Value::Func(..) => TypstValueEnum::Func,
        Value::Label(..) => TypstValueEnum::Label,
        Value::Bool(..) => TypstValueEnum::Bool,
        Value::Int(..) => TypstValueEnum::Int,
        Value::Float(..) => TypstValueEnum::Float,
        Value::Decimal(..) => TypstValueEnum::Decimal,
        Value::Length(..) => TypstValueEnum::Length,
        Value::Angle(..) => TypstValueEnum::Angle,
        Value::Ratio(..) => TypstValueEnum::Ratio,
        Value::Relative(..) => TypstValueEnum::Relative,
        Value::Fraction(..) => TypstValueEnum::Fraction,
        Value::Color(..) => TypstValueEnum::Color,
        Value::Gradient(..) => TypstValueEnum::Gradient,
        Value::Tiling(..) => TypstValueEnum::Tiling,
        Value::Symbol(..) => TypstValueEnum::Symbol,
        Value::Version(..) => TypstValueEnum::Version,
        Value::Bytes(..) => TypstValueEnum::Bytes,
        Value::Datetime(..) => TypstValueEnum::Datetime,
        Value::Duration(..) => TypstValueEnum::Duration,
        Value::Content(..) => TypstValueEnum::Content,
        Value::Styles(..) => TypstValueEnum::Styles,
        Value::Type(..) => TypstValueEnum::Type,
        Value::Dyn(..) => TypstValueEnum::Dyn,
    }
}

fn ptr_cmp<T: PartialEq>(x: &T, y: &T) -> std::cmp::Ordering {
    if x == y {
        std::cmp::Ordering::Equal
    } else {
        let x = std::ptr::from_ref(x);
        let y = std::ptr::from_ref(y);
        x.cmp(&y)
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum TypstValueEnum {
    Str,
    None,
    Auto,
    Array,
    Args,
    Dict,
    Module,
    Func,
    Label,
    Bool,
    Int,
    Float,
    Decimal,
    Length,
    Angle,
    Ratio,
    Relative,
    Fraction,
    Color,
    Gradient,
    Tiling,
    Symbol,
    Version,
    Bytes,
    Datetime,
    Duration,
    Content,
    Styles,
    Type,
    Dyn,
}

impl InsTy {
    /// Creates an instance.
    pub fn new(val: Value) -> Interned<Self> {
        Self { val, syntax: None }.into()
    }

    /// Creates an instance with a sapn.
    pub fn new_at(val: Value, span: Span) -> Interned<Self> {
        let mut name = SyntaxNode::leaf(SyntaxKind::Ident, "");
        name.synthesize(span);
        Interned::new(Self {
            val,
            syntax: Some(Interned::new(TypeSource {
                name_node: name,
                name_repr: OnceLock::new(),
                doc: "".into(),
            })),
        })
    }

    /// Creates an instance with a documentation string.
    pub fn new_doc(val: Value, doc: impl Into<StrRef>) -> Interned<Self> {
        Interned::new(Self {
            val,
            syntax: Some(Interned::new(TypeSource {
                name_node: SyntaxNode::default(),
                name_repr: OnceLock::new(),
                doc: doc.into(),
            })),
        })
    }

    /// Gets the span of the instance.
    pub fn span(&self) -> Span {
        self.syntax
            .as_ref()
            .map(|source| source.name_node.span())
            .or_else(|| {
                Some(match &self.val {
                    Value::Func(func) => func.span(),
                    Value::Args(args) => args.span,
                    Value::Content(content) => content.span(),
                    _ => return None,
                })
            })
            .unwrap_or_else(Span::detached)
    }
}

/// Describes a function parameter attribute.
#[derive(
    Debug, Clone, Copy, Hash, Serialize, Deserialize, Default, PartialEq, Eq, PartialOrd, Ord,
)]
pub struct ParamAttrs {
    /// Whether the parameter is positional.
    pub positional: bool,
    /// Whether the parameter is named.
    ///
    /// Can be true even if `positional` is true if the parameter can be given
    /// in both variants.
    pub named: bool,
    /// Whether the parameter can be given any number of times.
    pub variadic: bool,
    /// Whether the parameter is settable with a set rule.
    pub settable: bool,
}

impl ParamAttrs {
    /// Creates a positional parameter attribute.
    pub fn positional() -> ParamAttrs {
        ParamAttrs {
            positional: true,
            named: false,
            variadic: false,
            settable: false,
        }
    }

    /// Creates a named parameter attribute.
    pub fn named() -> ParamAttrs {
        ParamAttrs {
            positional: false,
            named: true,
            variadic: false,
            settable: false,
        }
    }

    /// Creates a variadic parameter attribute.
    pub fn variadic() -> ParamAttrs {
        ParamAttrs {
            positional: true,
            named: false,
            variadic: true,
            settable: false,
        }
    }
}

impl From<&ParamInfo> for ParamAttrs {
    fn from(param: &ParamInfo) -> Self {
        ParamAttrs {
            positional: param.positional,
            named: param.named,
            variadic: param.variadic,
            settable: param.settable,
        }
    }
}

/// Describes a parameter type.
#[derive(Debug, Clone, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ParamTy {
    /// The name of the parameter.
    pub name: StrRef,
    /// The docstring of the parameter.
    pub docs: Option<EcoString>,
    /// The default value of the variable.
    pub default: Option<EcoString>,
    /// The type of the parameter.
    pub ty: Ty,
    /// The attributes of the parameter.
    pub attrs: ParamAttrs,
}

impl ParamTy {
    /// Creates an untyped field type.
    pub fn new_untyped(name: StrRef, attrs: ParamAttrs) -> Interned<Self> {
        Self::new(Ty::Any, name, attrs)
    }

    /// Creates a typed field type.
    pub fn new(ty: Ty, name: StrRef, attrs: ParamAttrs) -> Interned<Self> {
        Interned::new(Self {
            name,
            ty,
            docs: None,
            default: None,
            attrs,
        })
    }
}

/// A type variable.
#[derive(Hash, Clone, PartialEq, Eq)]
pub struct TypeVar {
    /// The name of the type variable.
    pub name: StrRef,
    /// The definition id of the type variable.
    pub def: DeclExpr,
}

impl Ord for TypeVar {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // todo: buggy
        self.def.cmp(&other.def)
    }
}

impl TypeVar {
    /// Low-performance comparison but it is free from the concurrency issue.
    /// This is only used for making stable test snapshots.
    pub fn strict_cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.def.strict_cmp(&other.def)
    }
}

impl PartialOrd for TypeVar {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Debug for TypeVar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{}", self.name)
    }
}

impl TypeVar {
    /// Creates a type variable.
    pub fn new(name: StrRef, def: DeclExpr) -> Interned<Self> {
        Interned::new(Self { name, def })
    }

    /// Gets the name of the type variable.
    pub fn name(&self) -> StrRef {
        self.name.clone()
    }
}

/// A record type.
#[derive(Hash, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RecordTy {
    /// The names of the fields.
    pub names: Interned<NameBone>,
    /// The types of the fields.
    pub types: Interned<Vec<Ty>>,
}

impl RecordTy {
    /// Shapes the fields of a record.
    pub fn shape_fields(mut fields: Vec<(StrRef, Ty)>) -> (NameBone, Vec<Ty>) {
        fields.sort_by(|a, b| a.0.cmp(&b.0));
        let names = NameBone {
            names: fields.iter().map(|(name, _)| name.clone()).collect(),
        };
        let types = fields.into_iter().map(|(_, ty)| ty).collect::<Vec<_>>();

        (names, types)
    }

    /// Creates a record type.
    pub fn new(fields: Vec<(StrRef, Ty)>) -> Interned<Self> {
        let (names, types) = Self::shape_fields(fields);
        Interned::new(Self {
            types: Interned::new(types),
            names: Interned::new(names),
        })
    }
}

impl TypeInterface for RecordTy {
    fn bone(&self) -> &Interned<NameBone> {
        &self.names
    }

    fn field_by_bone_offset(&self, idx: usize) -> Option<&Ty> {
        self.types.get(idx)
    }

    fn interface(&self) -> impl Iterator<Item = (&StrRef, &Ty)> {
        self.names.names.iter().zip(self.types.iter())
    }
}

impl fmt::Debug for RecordTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("{")?;
        interpersed(
            f,
            self.interface()
                .map(|(name, ty)| TypeSigParam::Named(name, ty)),
        )?;
        f.write_str("}")
    }
}

/// A typst function type.
#[derive(Hash, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SigTy {
    /// The input types of the function.
    pub inputs: Interned<Vec<Ty>>,
    /// The return (body) type of the function.
    pub body: Option<Ty>,
    /// The name bone of the named parameters.
    pub names: Interned<NameBone>,
    /// The index of the first named parameter.
    pub name_started: u32,
    /// Whether the function has a spread left parameter.
    pub spread_left: bool,
    /// Whether the function has a spread right parameter.
    pub spread_right: bool,
}

impl SigTy {
    /// Creates an function that accepts any arguments: `(a, b: c, ..d)`
    pub fn any() -> Interned<SigTy> {
        let rest = Ty::Array(Interned::new(Ty::Any));
        Interned::new(Self {
            inputs: Interned::new(vec![rest]),
            body: Some(Ty::Any),
            names: NameBone::empty(),
            name_started: 0,
            spread_left: false,
            spread_right: true,
        })
    }

    /// Creates an array constructor: `(a)`
    #[comemo::memoize]
    pub fn array_cons(elem: Ty, anyify: bool) -> Interned<SigTy> {
        let rest = Ty::Array(Interned::new(elem.clone()));
        let ret = if anyify { Ty::Any } else { rest.clone() };
        Interned::new(Self {
            inputs: Interned::new(vec![rest]),
            body: Some(ret),
            names: NameBone::empty(),
            name_started: 0,
            spread_left: false,
            spread_right: true,
        })
    }

    /// Creates a unary constructor: `(a) => b`
    #[comemo::memoize]
    pub fn unary(inp: Ty, ret: Ty) -> Interned<SigTy> {
        Interned::new(Self {
            inputs: Interned::new(vec![inp]),
            body: Some(ret),
            names: NameBone::empty(),
            name_started: 1,
            spread_left: false,
            spread_right: false,
        })
    }

    /// Creates a tuple constructor: `(a, b, c)`
    #[comemo::memoize]
    pub fn tuple_cons(elems: Interned<Vec<Ty>>, anyify: bool) -> Interned<SigTy> {
        let ret = if anyify {
            Ty::Any
        } else {
            Ty::Tuple(elems.clone())
        };
        let name_started = elems.len() as u32;
        Interned::new(Self {
            inputs: elems,
            body: Some(ret),
            names: NameBone::empty(),
            name_started,
            spread_left: false,
            spread_right: false,
        })
    }

    /// Creates a dictionary constructor: `(a: b, c: d)`
    #[comemo::memoize]
    pub fn dict_cons(named: &Interned<RecordTy>, anyify: bool) -> Interned<SigTy> {
        let ret = if anyify {
            Ty::Any
        } else {
            Ty::Dict(named.clone())
        };

        Interned::new(Self {
            inputs: named.types.clone(),
            body: Some(ret),
            names: named.names.clone(),
            name_started: 0,
            spread_left: false,
            spread_right: false,
        })
    }

    /// Sets the return type of the function.
    pub fn with_body(mut self, res_ty: Ty) -> Self {
        self.body = Some(res_ty);
        self
    }

    /// Creates a function type.
    pub fn new(
        pos: impl ExactSizeIterator<Item = Ty>,
        named: impl IntoIterator<Item = (StrRef, Ty)>,
        rest_left: Option<Ty>,
        rest_right: Option<Ty>,
        ret_ty: Option<Ty>,
    ) -> Self {
        let named = named.into_iter().collect::<Vec<_>>();
        let (names, mut named_types) = RecordTy::shape_fields(named);
        let spread_left = rest_left.is_some();
        let spread_right = rest_right.is_some();

        let name_started = if spread_right { 1 } else { 0 } + named_types.len();
        let mut types = Vec::with_capacity(
            pos.len() + named_types.len() + spread_left as usize + spread_right as usize,
        );
        types.extend(pos);
        types.append(&mut named_types);
        types.extend(rest_left);
        types.extend(rest_right);

        let name_started = (types.len() - name_started) as u32;

        Self {
            inputs: Interned::new(types),
            body: ret_ty,
            names: Interned::new(names),
            name_started,
            spread_left,
            spread_right,
        }
    }
}

impl Default for SigTy {
    fn default() -> Self {
        Self {
            inputs: Interned::new(Vec::new()),
            body: None,
            names: NameBone::empty(),
            name_started: 0,
            spread_left: false,
            spread_right: false,
        }
    }
}

impl TypeInterface for SigTy {
    fn bone(&self) -> &Interned<NameBone> {
        &self.names
    }

    fn interface(&self) -> impl Iterator<Item = (&StrRef, &Ty)> {
        let names = self.names.names.iter();
        let types = self.inputs.iter().skip(self.name_started as usize);
        names.zip(types)
    }

    fn field_by_bone_offset(&self, offset: usize) -> Option<&Ty> {
        self.inputs.get(offset + self.name_started as usize)
    }
}

impl SigTy {
    /// Gets the input types of the function.
    pub fn inputs(&self) -> impl Iterator<Item = &Ty> {
        self.inputs.iter()
    }

    /// Gets the positional parameters of the function.
    pub fn positional_params(&self) -> impl ExactSizeIterator<Item = &Ty> {
        self.inputs.iter().take(self.name_started as usize)
    }

    /// Gets the parameter at the given index.
    pub fn pos(&self, idx: usize) -> Option<&Ty> {
        (idx < self.name_started as usize)
            .then_some(())
            .and_then(|_| self.inputs.get(idx))
    }

    /// Gets the parameter or the rest parameter at the given index.
    pub fn pos_or_rest(&self, idx: usize) -> Option<Ty> {
        let nth = self.pos(idx).cloned();
        nth.or_else(|| {
            let rest_idx = || idx.saturating_sub(self.positional_params().len());

            let rest_ty = self.rest_param()?;
            match rest_ty {
                Ty::Array(ty) => Some(ty.as_ref().clone()),
                Ty::Tuple(tys) => tys.get(rest_idx()).cloned(),
                _ => None,
            }
        })
    }

    /// Gets the named parameters of the function.
    pub fn named_params(&self) -> impl ExactSizeIterator<Item = (&StrRef, &Ty)> {
        let named_names = self.names.names.iter();
        let named_types = self.inputs.iter().skip(self.name_started as usize);

        named_names.zip(named_types)
    }

    /// Gets the named parameter by given name.
    pub fn named(&self, name: &StrRef) -> Option<&Ty> {
        let idx = self.names.find(name)?;
        self.inputs.get(idx + self.name_started as usize)
    }

    /// Gets the rest parameter of the function.
    pub fn rest_param(&self) -> Option<&Ty> {
        if self.spread_right {
            self.inputs.last()
        } else {
            None
        }
    }

    /// Matches the function type with the given arguments.
    pub fn matches<'a>(
        &'a self,
        args: &'a SigTy,
        with: Option<&'a Vec<Interned<SigTy>>>,
    ) -> impl Iterator<Item = (&'a Ty, &'a Ty)> + 'a {
        let with_len = with
            .map(|w| w.iter().map(|w| w.positional_params().len()).sum::<usize>())
            .unwrap_or(0);

        let sig_pos = self.positional_params();
        let arg_pos = args.positional_params();

        let sig_rest = self.rest_param();
        let arg_rest = args.rest_param();

        let max_len = sig_pos.len().max(with_len + arg_pos.len())
            + if sig_rest.is_some() && arg_rest.is_some() {
                1
            } else {
                0
            };

        let arg_pos = with
            .into_iter()
            .flat_map(|w| w.iter().rev().map(|w| w.positional_params()))
            .flatten()
            .chain(arg_pos);

        let sig_stream = sig_pos.chain(sig_rest.into_iter().cycle()).take(max_len);
        let arg_stream = arg_pos.chain(arg_rest.into_iter().cycle()).take(max_len);

        let pos = sig_stream.zip(arg_stream);
        let common_ifaces = with
            .map(|args_all| args_all.iter().rev())
            .into_iter()
            .flatten()
            .flat_map(|args| self.common_iface_fields(args))
            .chain(self.common_iface_fields(args));
        let named = common_ifaces.map(|(_, l, r)| (l, r));

        pos.chain(named)
    }
}

impl fmt::Debug for SigTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("(")?;
        let pos = self.positional_params().map(TypeSigParam::Pos);
        let named = self
            .named_params()
            .map(|(name, ty)| TypeSigParam::Named(name, ty));
        let rest = self.rest_param().map(TypeSigParam::Rest);
        interpersed(f, pos.chain(named).chain(rest))?;
        f.write_str(") => ")?;
        if let Some(ret) = &self.body {
            ret.fmt(f)?;
        } else {
            f.write_str("any")?;
        }
        Ok(())
    }
}

/// A function argument type.
pub type ArgsTy = SigTy;

/// A pattern type.
pub type PatternTy = SigTy;

/// A type with partially applied arguments.
#[derive(Hash, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SigWithTy {
    /// The signature of the function.
    pub sig: TyRef,
    /// The arguments applied to the function.
    pub with: Interned<ArgsTy>,
}

impl SigWithTy {
    /// Creates a type with applied arguments.
    pub fn new(sig: TyRef, with: Interned<ArgsTy>) -> Interned<Self> {
        Interned::new(Self { sig, with })
    }
}

impl fmt::Debug for SigWithTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}.with({:?})", self.sig, self.with)
    }
}

/// A field selection type.
#[derive(Hash, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SelectTy {
    /// The type to select from.
    pub ty: TyRef,
    /// The field to select
    pub select: StrRef,
}

impl SelectTy {
    /// Creates a field selection type.
    pub fn new(ty: TyRef, select: StrRef) -> Interned<Self> {
        Interned::new(Self { ty, select })
    }
}

impl fmt::Debug for SelectTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}.{}", RefDebug(&self.ty), self.select)
    }
}

/// A unary operation type.
#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct TypeUnary {
    /// The operand of the unary operation.
    pub lhs: Ty,
    /// The kind of the unary operation
    pub op: UnaryOp,
}

impl PartialOrd for TypeUnary {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TypeUnary {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let op_as_int = self.op as u8;
        let other_op_as_int = other.op as u8;
        op_as_int
            .cmp(&other_op_as_int)
            .then_with(|| self.lhs.cmp(&other.lhs))
    }
}

impl TypeUnary {
    /// Creates a unary operation type.
    pub fn new(op: UnaryOp, lhs: Ty) -> Interned<Self> {
        Interned::new(Self { lhs, op })
    }

    /// Gets the operands of the unary operation.
    pub fn operands(&self) -> [&Ty; 1] {
        [&self.lhs]
    }
}

/// The kind of binary operation.
pub type BinaryOp = ast::BinOp;

/// A binary operation type.
#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct TypeBinary {
    /// The operands of the binary operation.
    pub operands: (Ty, Ty),
    /// The kind of the binary operation.
    pub op: BinaryOp,
}

impl PartialOrd for TypeBinary {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for TypeBinary {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        let op_as_int = self.op as u8;
        let other_op_as_int = other.op as u8;
        op_as_int
            .cmp(&other_op_as_int)
            .then_with(|| self.operands.cmp(&other.operands))
    }
}

impl TypeBinary {
    /// Creates a binary operation type.
    pub fn new(op: BinaryOp, lhs: Ty, rhs: Ty) -> Interned<Self> {
        Interned::new(Self {
            operands: (lhs, rhs),
            op,
        })
    }

    /// Gets the operands of the binary operation.
    pub fn operands(&self) -> [&Ty; 2] {
        [&self.operands.0, &self.operands.1]
    }
}

/// A conditional type.
/// `if t1 then t2 else t3`
#[derive(Debug, Hash, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct IfTy {
    /// The condition.
    pub cond: TyRef,
    /// The type when the condition is true.
    pub then: TyRef,
    /// The type when the condition is false.
    pub else_: TyRef,
}

impl IfTy {
    /// Creates a conditional type.
    pub fn new(cond: TyRef, then: TyRef, else_: TyRef) -> Interned<Self> {
        Interned::new(Self { cond, then, else_ })
    }
}

/// The type information on a group of syntax structures (typing).
#[derive(Default)]
pub struct TypeInfo {
    /// Whether the typing is valid.
    pub valid: bool,
    /// The belonging file id.
    pub fid: Option<FileId>,
    /// The used revision.
    pub revision: usize,
    /// The exported types.
    pub exports: FxHashMap<StrRef, Ty>,
    /// The typing on definitions.
    pub vars: FxHashMap<DeclExpr, TypeVarBounds>,
    /// The checked documentation of definitions.
    pub var_docs: FxHashMap<DeclExpr, Arc<UntypedDefDocs>>,
    /// The local binding of the type variable.
    pub local_binds: snapshot_map::SnapshotMap<DeclExpr, Ty>,
    /// The typing on syntax structures.
    pub mapping: FxHashMap<Span, FxHashSet<Ty>>,
    /// The cache to canonicalize types.
    pub(super) cano_cache: Mutex<TypeCanoStore>,
}

impl Hash for TypeInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.valid.hash(state);
        self.fid.hash(state);
        self.revision.hash(state);
    }
}

impl TyCtx for TypeInfo {
    fn global_bounds(&self, var: &Interned<TypeVar>, _pol: bool) -> Option<DynTypeBounds> {
        let v = self.vars.get(&var.def)?;
        Some(v.bounds.bounds().read().clone())
    }

    fn local_bind_of(&self, var: &Interned<TypeVar>) -> Option<Ty> {
        self.local_binds.get(&var.def).cloned()
    }
}

impl TypeInfo {
    /// Gets the type of a syntax structure.
    pub fn type_of_span(&self, site: Span) -> Option<Ty> {
        self.mapping
            .get(&site)
            .cloned()
            .map(|types| Ty::from_types(types.into_iter()))
    }

    // todo: distinguish at least, at most
    /// Witnesses a lower-bound type on a syntax structure.
    pub fn witness_at_least(&mut self, site: Span, ty: Ty) {
        Self::witness_(site, ty, &mut self.mapping);
    }
    /// Witnesses a upper-bound type on a syntax structure.
    pub fn witness_at_most(&mut self, site: Span, ty: Ty) {
        Self::witness_(site, ty, &mut self.mapping);
    }

    /// Witnesses a type.
    pub fn witness_(site: Span, ty: Ty, mapping: &mut FxHashMap<Span, FxHashSet<Ty>>) {
        if site.is_detached() {
            return;
        }

        // todo: intersect/union
        mapping.entry(site).or_default().insert(ty);
    }

    /// Converts a type to a type with bounds.
    pub fn to_bounds(&self, def: Ty) -> DynTypeBounds {
        let mut store = DynTypeBounds::default();
        match def {
            Ty::Var(v) => {
                let w = self.vars.get(&v.def).unwrap();
                match &w.bounds {
                    FlowVarKind::Strong(bounds) | FlowVarKind::Weak(bounds) => {
                        let w = bounds.read();
                        for bound in w.lbs.iter() {
                            store.lbs.insert_mut(bound.clone());
                        }
                        for bound in w.ubs.iter() {
                            store.ubs.insert_mut(bound.clone());
                        }
                    }
                }
            }
            Ty::Let(bounds) => {
                for bound in bounds.lbs.iter() {
                    store.lbs.insert_mut(bound.clone());
                }
                for bound in bounds.ubs.iter() {
                    store.ubs.insert_mut(bound.clone());
                }
            }
            _ => {
                store.ubs.insert_mut(def);
            }
        }

        store
    }
}

impl TyCtxMut for TypeInfo {
    type Snap = ena::undo_log::Snapshot;

    fn start_scope(&mut self) -> Self::Snap {
        self.local_binds.snapshot()
    }

    fn end_scope(&mut self, snap: Self::Snap) {
        self.local_binds.rollback_to(snap);
    }

    fn bind_local(&mut self, var: &Interned<TypeVar>, ty: Ty) {
        self.local_binds.insert(var.def.clone(), ty);
    }

    fn type_of_func(&mut self, _func: &typst::foundations::Func) -> Option<Interned<SigTy>> {
        None
    }

    fn type_of_value(&mut self, _val: &Value) -> Ty {
        Ty::Any
    }

    fn check_module_item(&mut self, _module: FileId, _key: &StrRef) -> Option<Ty> {
        None
    }
}

/// A type variable bounds.
#[derive(Clone)]
pub struct TypeVarBounds {
    /// The type variable representation.
    pub var: Interned<TypeVar>,
    /// The bounds of the type variable.
    pub bounds: FlowVarKind,
}

impl fmt::Debug for TypeVarBounds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.var)
    }
}

impl TypeVarBounds {
    /// Creates a type variable bounds.
    pub fn new(var: TypeVar, init: DynTypeBounds) -> Self {
        Self {
            var: Interned::new(var),
            bounds: FlowVarKind::Strong(Arc::new(RwLock::new(init.clone()))),
        }
    }

    /// Gets the name of the type variable.
    pub fn name(&self) -> &StrRef {
        &self.var.name
    }

    /// Gets self as a type.
    pub fn as_type(&self) -> Ty {
        Ty::Var(self.var.clone())
    }

    /// Slightly closes the type variable.
    pub fn weaken(&mut self) {
        match &self.bounds {
            FlowVarKind::Strong(w) => {
                self.bounds = FlowVarKind::Weak(w.clone());
            }
            FlowVarKind::Weak(_) => {}
        }
    }
}

/// A type variable bounds.
#[derive(Clone)]
pub enum FlowVarKind {
    /// A type variable that receives both types and values (type instances).
    Strong(Arc<RwLock<DynTypeBounds>>),
    /// A type variable that receives only types.
    /// The received values will be lifted to types.
    Weak(Arc<RwLock<DynTypeBounds>>),
}

impl FlowVarKind {
    /// Gets the bounds of the type variable.
    pub fn bounds(&self) -> &RwLock<DynTypeBounds> {
        match self {
            FlowVarKind::Strong(w) | FlowVarKind::Weak(w) => w,
        }
    }
}

/// A cache to canonicalize types.
#[derive(Default)]
pub(super) struct TypeCanoStore {
    /// Maps a type to its canonical form.
    pub cano_cache: FxHashMap<(Ty, bool), Ty>,
    /// Maps a local type to its canonical form.
    pub cano_local_cache: FxHashMap<(DeclExpr, bool), Ty>,
    /// The negative bounds of a type variable.
    pub negatives: FxHashSet<DeclExpr>,
    /// The positive bounds of a type variable.
    pub positives: FxHashSet<DeclExpr>,
}

impl_internable!(Ty,);
impl_internable!(InsTy,);
impl_internable!(ParamTy,);
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
impl_internable!(PackageId,);
impl_internable!((Ty, Ty),);

struct RefDebug<'a>(&'a Ty);

impl fmt::Debug for RefDebug<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Ty::Var(v) => write!(f, "@v{:?}", v.name()),
            _ => write!(f, "{:?}", self.0),
        }
    }
}

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
    use insta::{assert_debug_snapshot, assert_snapshot};

    use crate::ty::tests::*;

    #[test]
    fn test_ty_size() {
        use super::*;
        assert!(size_of::<Ty>() <= size_of::<usize>() * 2);
    }

    #[test]
    fn test_ty() {
        use super::*;
        let ty = Ty::Builtin(BuiltinTy::Clause);
        let ty_ref = TyRef::new(ty.clone());
        assert_debug_snapshot!(ty_ref, @"Clause");
    }

    #[test]
    fn test_sig_matches() {
        use super::*;

        fn matches(
            sig: Interned<SigTy>,
            args: Interned<SigTy>,
            with: Option<Vec<Interned<ArgsTy>>>,
        ) -> String {
            let res = sig.matches(&args, with.as_ref()).collect::<Vec<_>>();
            format!("{res:?}")
        }

        assert_snapshot!(matches(literal_sig!(p1), literal_sig!(q1), None), @"[(@p1, @q1)]");
        assert_snapshot!(matches(literal_sig!(p1, p2), literal_sig!(q1), None), @"[(@p1, @q1)]");
        assert_snapshot!(matches(literal_sig!(p1, p2), literal_sig!(q1, q2), None), @"[(@p1, @q1), (@p2, @q2)]");
        assert_snapshot!(matches(literal_sig!(p1), literal_sig!(q1, q2), None), @"[(@p1, @q1)]");

        assert_snapshot!(matches(literal_sig!(p1, ...r1), literal_sig!(q1), None), @"[(@p1, @q1)]");
        assert_snapshot!(matches(literal_sig!(p1, ...r1), literal_sig!(q1, q2), None), @"[(@p1, @q1), (@r1, @q2)]");
        assert_snapshot!(matches(literal_sig!(p1, ...r1), literal_sig!(q1, q2, q3), None), @"[(@p1, @q1), (@r1, @q2), (@r1, @q3)]");
        assert_snapshot!(matches(literal_sig!(...r1), literal_sig!(q1, q2), None), @"[(@r1, @q1), (@r1, @q2)]");

        assert_snapshot!(matches(literal_sig!(p1), literal_sig!(q1, ...s2), None), @"[(@p1, @q1)]");
        assert_snapshot!(matches(literal_sig!(p1, p2), literal_sig!(q1, ...s2), None), @"[(@p1, @q1), (@p2, @s2)]");
        assert_snapshot!(matches(literal_sig!(p1, p2, p3), literal_sig!(q1, ...s2), None), @"[(@p1, @q1), (@p2, @s2), (@p3, @s2)]");
        assert_snapshot!(matches(literal_sig!(p1, p2), literal_sig!(...s2), None), @"[(@p1, @s2), (@p2, @s2)]");

        assert_snapshot!(matches(literal_sig!(p1, ...r1), literal_sig!(q1, ...s2), None), @"[(@p1, @q1), (@r1, @s2)]");
        assert_snapshot!(matches(literal_sig!(...r1), literal_sig!(q1, ...s2), None), @"[(@r1, @q1), (@r1, @s2)]");
        assert_snapshot!(matches(literal_sig!(p1, ...r1), literal_sig!(...s2), None), @"[(@p1, @s2), (@r1, @s2)]");
        assert_snapshot!(matches(literal_sig!(...r1), literal_sig!(...s2), None), @"[(@r1, @s2)]");

        assert_snapshot!(matches(literal_sig!(p0, p1, ...r1), literal_sig!(q1, ...s2), None), @"[(@p0, @q1), (@p1, @s2), (@r1, @s2)]");
        assert_snapshot!(matches(literal_sig!(p0, p1, ...r1), literal_sig!(...s2), None), @"[(@p0, @s2), (@p1, @s2), (@r1, @s2)]");

        assert_snapshot!(matches(literal_sig!(p1, ...r1), literal_sig!(q0, q1, ...s2), None), @"[(@p1, @q0), (@r1, @q1), (@r1, @s2)]");
        assert_snapshot!(matches(literal_sig!(...r1), literal_sig!(q0, q1, ...s2), None), @"[(@r1, @q0), (@r1, @q1), (@r1, @s2)]");

        assert_snapshot!(matches(literal_sig!(p1 !u1: w1), literal_sig!(q1 !u1: w2), None), @"[(@p1, @q1), (@w1, @w2)]");
        assert_snapshot!(matches(literal_sig!(p1 !u1: w1, ...r1), literal_sig!(q1 !u1: w2), None), @"[(@p1, @q1), (@w1, @w2)]");
        assert_snapshot!(matches(literal_sig!(p1 !u1: w1), literal_sig!(q1 !u1: w2, ...s2), None), @"[(@p1, @q1), (@w1, @w2)]");
        assert_snapshot!(matches(literal_sig!(p1 !u1: w1, ...r1), literal_sig!(q1 !u1: w2, ...s2), None), @"[(@p1, @q1), (@r1, @s2), (@w1, @w2)]");

        assert_snapshot!(matches(literal_sig!(), literal_sig!(!u1: w2), None), @"[]");
        assert_snapshot!(matches(literal_sig!(!u1: w1), literal_sig!(), None), @"[]");
        assert_snapshot!(matches(literal_sig!(!u1: w1), literal_sig!(!u1: w2), None), @"[(@w1, @w2)]");
        assert_snapshot!(matches(literal_sig!(!u1: w1), literal_sig!(!u2: w2), None), @"[]");
        assert_snapshot!(matches(literal_sig!(!u2: w1), literal_sig!(!u1: w2), None), @"[]");
        assert_snapshot!(matches(literal_sig!(!u1: w1, !u2: w3), literal_sig!(!u1: w2, !u2: w4), None), @"[(@w1, @w2), (@w3, @w4)]");
        assert_snapshot!(matches(literal_sig!(!u1: w1, !u2: w3), literal_sig!(!u2: w2, !u1: w4), None), @"[(@w1, @w4), (@w3, @w2)]");
        assert_snapshot!(matches(literal_sig!(!u2: w1), literal_sig!(!u1: w2, !u2: w4), None), @"[(@w1, @w4)]");
        assert_snapshot!(matches(literal_sig!(!u1: w1, !u2: w2), literal_sig!(!u2: w4), None), @"[(@w2, @w4)]");

        assert_snapshot!(matches(literal_sig!(p1 !u1: w1, !u2: w2), literal_sig!(q1), Some(vec![
            literal_sig!(!u2: w6),
        ])), @"[(@p1, @q1), (@w2, @w6)]");
        assert_snapshot!(matches(literal_sig!(p1 !u1: w1, !u2: w2), literal_sig!(q1 !u2: w4), Some(vec![
            literal_sig!(!u2: w5),
        ])), @"[(@p1, @q1), (@w2, @w5), (@w2, @w4)]");
        assert_snapshot!(matches(literal_sig!(p1 !u1: w1, !u2: w2), literal_sig!(q1 ), Some(vec![
            literal_sig!(!u2: w7),
            literal_sig!(!u2: w8),
        ])), @"[(@p1, @q1), (@w2, @w8), (@w2, @w7)]");
        assert_snapshot!(matches(literal_sig!(p1, p2, p3), literal_sig!(q1), Some(vec![
            literal_sig!(q2),
            literal_sig!(q3),
        ])), @"[(@p1, @q3), (@p2, @q2), (@p3, @q1)]");
    }
}
