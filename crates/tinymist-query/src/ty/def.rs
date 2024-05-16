//! Name Convention:
//! - `TypeXXX`: abstracted types or clauses
//! - `XXTy`: concrete types

use core::fmt;
use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    hash::{Hash, Hasher},
    sync::Arc,
};

use ecow::EcoVec;
use once_cell::sync::OnceCell;
use parking_lot::{Mutex, RwLock};
use reflexo::vector::ir::DefId;
use typst::{
    foundations::Value,
    syntax::{ast, Span, SyntaxKind, SyntaxNode},
};

use crate::{
    adt::interner::{impl_internable, Interned},
    analysis::BuiltinTy,
};

/// A reference to the interned type
pub(super) type TyRef = Interned<Ty>;
/// A reference to the interned string
pub(super) type StrRef = Interned<str>;

/// All possible types in tinymist
#[derive(Hash, Clone, PartialEq, Eq)]
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
    /// A field type
    Field(Interned<FieldTy>),

    // Combination Types
    /// A union type, whose negation is intersection type.
    /// `t := t1 | t2 | ... | tn, t^- := t1 & t2 & ... & tn`
    Union(Interned<Vec<Ty>>),
    /// A frozen type variable
    /// `t :> t1 | t2 | ... | tn <: f1 & f2 & ... & fn`
    Let(Interned<TypeBounds>),
    /// An opening type variable owing bounds
    Var(Interned<TypeVar>),

    // Composite Types
    /// A typst dictionary type
    Dict(Interned<RecordTy>),
    /// An array type
    Array(TyRef),
    /// A tuple type
    /// Note: may contains spread types
    Tuple(Interned<Vec<Ty>>),
    /// A function type
    Func(Interned<SigTy>),
    /// An argument type
    Args(Interned<ArgsTy>),

    // Type operations
    /// A partially applied function type
    With(Interned<SigWithTy>),
    /// Select a field from a type
    Select(Interned<SelectTy>),
    /// A unary operation
    Unary(Interned<TypeUnary>),
    /// A binary operation
    Binary(Interned<TypeBinary>),
    /// A conditional type
    If(Interned<IfTy>),
}

impl fmt::Debug for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Any => f.write_str("Any"),
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
            Ty::Var(v) => v.fmt(f),
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

impl Ty {
    /// Whether the type is a dictionary type
    pub fn is_dict(&self) -> bool {
        matches!(self, Ty::Dict(..))
    }

    /// Create a union type from an iterator of types
    pub fn from_types(e: impl ExactSizeIterator<Item = Ty>) -> Self {
        if e.len() == 0 {
            Ty::Any
        } else if e.len() == 1 {
            let mut e = e;
            e.next().unwrap()
        } else {
            Self::iter_union(e)
        }
    }

    /// Create a union type from an iterator of types
    pub fn iter_union(e: impl IntoIterator<Item = Ty>) -> Self {
        Ty::Union(Interned::new(e.into_iter().collect()))
    }

    /// Create an undefined type (which will emit an error)
    /// A that type is annotated if the syntax structure causes an type error
    pub const fn undef() -> Self {
        Ty::Builtin(BuiltinTy::Undef)
    }
}

/// A function parameter type
pub enum TypeSigParam<'a> {
    /// A positional parameter
    Pos(&'a Ty),
    /// A named parameter
    Named(&'a StrRef, &'a Ty),
    /// A rest parameter (spread right)
    Rest(&'a Ty),
}

impl fmt::Debug for TypeSigParam<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TypeSigParam::Pos(ty) => write!(f, "{ty:?}"),
            TypeSigParam::Named(name, ty) => write!(f, "{name:?}: {ty:?}"),
            TypeSigParam::Rest(ty) => write!(f, "...: {ty:?}[]"),
        }
    }
}

/// The syntax source (definition) of a type node
/// todo: whether we should store them in the type node
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeSource {
    /// A name node with span
    pub name_node: SyntaxNode,
    /// A lazy evaluated name
    pub name_repr: OnceCell<StrRef>,
    /// Attached documentation
    pub doc: StrRef,
}

impl Hash for TypeSource {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name_node.hash(state);
        self.doc.hash(state);
    }
}

impl TypeSource {
    /// Get name of the type node
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

/// An ordered list of names
#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct NameBone {
    /// The names in the bone
    pub names: Box<[StrRef]>,
}

impl NameBone {
    /// Create an empty bone
    pub fn empty() -> Interned<Self> {
        Interned::new(Self {
            names: Box::new([]),
        })
    }
}

impl NameBone {
    /// Find the index of the name in the bone
    pub fn find(&self, name: &StrRef) -> Option<usize> {
        self.names.binary_search_by(|probe| probe.cmp(name)).ok()
    }
}

impl NameBone {
    /// Intersect the names of two bones
    pub fn intersect_enumerate<'a>(
        &'a self,
        rhs: &'a NameBone,
    ) -> impl Iterator<Item = (usize, usize)> + 'a {
        let mut lhs_iter = self.names.iter().enumerate();
        let mut rhs_iter = rhs.names.iter().enumerate();

        let mut lhs = lhs_iter.next();
        let mut rhs = rhs_iter.next();

        std::iter::from_fn(move || 'name_scanning: loop {
            if let (Some((i, lhs_key)), Some((j, rhs_key))) = (lhs, rhs) {
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
                        return Some((i, j));
                    }
                }
            }
            return None;
        })
    }
}

/// A frozen type variable (bounds of some type in program)
/// `t :> t1 | ... | tn <: f1 & ... & fn`
/// `  lbs------------- ubs-------------`
#[derive(Hash, Clone, PartialEq, Eq, Default)]
pub struct TypeBounds {
    /// The lower bounds
    pub lbs: EcoVec<Ty>,
    /// The upper bounds
    pub ubs: EcoVec<Ty>,
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

/// A common type kinds for those types that has fields (Abstracted record
/// type).
pub trait TypeInterface {
    /// Get the bone of a record.
    /// See [`NameBone`] for more details.
    fn bone(&self) -> &Interned<NameBone>;
    /// Iterate over the fields of a record.
    fn interface(&self) -> impl Iterator<Item = (&StrRef, &Ty)>;
    /// Get the field by bone offset.
    fn field_by_bone_offset(&self, i: usize) -> Option<&Ty>;
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

/// An instance of a typst type
#[derive(Debug, Hash, Clone, PartialEq)]
pub struct InsTy {
    /// The value of the instance
    pub val: Value,
    /// The syntax source of the instance
    pub syntax: Option<Interned<TypeSource>>,
}

/// There are some case that val is not Eq, but we make it Eq for simplicity
/// For example, a float instance which is NaN.
impl Eq for InsTy {}

impl InsTy {
    /// Create a instance
    pub fn new(val: Value) -> Interned<Self> {
        Self { val, syntax: None }.into()
    }

    /// Create a instance with a sapn
    pub fn new_at(val: Value, s: Span) -> Interned<Self> {
        let mut l = SyntaxNode::leaf(SyntaxKind::Ident, "");
        l.synthesize(s);
        Interned::new(Self {
            val,
            syntax: Some(Interned::new(TypeSource {
                name_node: l,
                name_repr: OnceCell::new(),
                doc: "".into(),
            })),
        })
    }
    /// Create a instance with a documentation string
    pub fn new_doc(val: Value, doc: impl Into<StrRef>) -> Interned<Self> {
        Interned::new(Self {
            val,
            syntax: Some(Interned::new(TypeSource {
                name_node: SyntaxNode::default(),
                name_repr: OnceCell::new(),
                doc: doc.into(),
            })),
        })
    }
}

/// A field type
#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct FieldTy {
    /// The name of the field
    pub name: StrRef,
    /// The type of the field
    pub field: Ty,
}

impl FieldTy {
    /// Create an untyped field type
    pub fn new_untyped(name: StrRef) -> Interned<Self> {
        Interned::new(Self {
            name,
            field: Ty::Any,
        })
    }
}

/// A type variable
#[derive(Hash, Clone, PartialEq, Eq)]
pub struct TypeVar {
    /// The name of the type variable
    pub name: StrRef,
    /// The definition id of the type variable
    pub def: DefId,
}

impl Ord for TypeVar {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.name
            .cmp(&other.name)
            .then_with(|| self.def.0.cmp(&other.def.0))
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
    /// Create a type variable
    pub fn new(name: StrRef, def: DefId) -> Interned<Self> {
        Interned::new(Self { name, def })
    }

    /// Get the name of the type variable
    pub fn name(&self) -> StrRef {
        self.name.clone()
    }

    /// Get the definition id of the type variable
    pub fn id(&self) -> DefId {
        self.def
    }
}

/// A record type
#[derive(Hash, Clone, PartialEq, Eq)]
pub struct RecordTy {
    /// The names of the fields
    pub names: Interned<NameBone>,
    /// The types of the fields
    pub types: Interned<Vec<Ty>>,
}

impl RecordTy {
    /// Shape the fields of a record
    pub fn shape_fields(mut fields: Vec<(StrRef, Ty, Span)>) -> (NameBone, Vec<Ty>) {
        fields.sort_by(|a, b| a.0.cmp(&b.0));
        let names = NameBone {
            names: fields.iter().map(|e| e.0.clone()).collect(),
        };
        let types = fields.into_iter().map(|(_, ty, _)| ty).collect::<Vec<_>>();

        (names, types)
    }

    /// Create a record type
    pub fn new(fields: Vec<(StrRef, Ty, Span)>) -> Interned<Self> {
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

    fn field_by_bone_offset(&self, i: usize) -> Option<&Ty> {
        self.types.get(i)
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

/// A typst function type
#[derive(Hash, Clone, PartialEq, Eq)]
pub struct SigTy {
    /// The input types of the function
    pub inputs: Interned<Vec<Ty>>,
    /// The return (body) type of the function
    pub body: Option<Ty>,
    /// The name bone of the named parameters
    pub names: Interned<NameBone>,
    /// The index of the first named parameter
    pub name_started: u32,
    /// Whether the function has a spread left parameter
    pub spread_left: bool,
    /// Whether the function has a spread right parameter
    pub spread_right: bool,
}

impl SigTy {
    /// Array constructor
    #[comemo::memoize]
    pub fn array_cons(elem: Ty, anyify: bool) -> Interned<SigTy> {
        let ret = if anyify {
            Ty::Any
        } else {
            Ty::Array(Interned::new(elem.clone()))
        };
        Interned::new(Self {
            inputs: Interned::new(vec![elem]),
            body: Some(ret),
            names: NameBone::empty(),
            name_started: 0,
            spread_left: false,
            spread_right: true,
        })
    }

    /// Dictionary constructor
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

    /// Create a function type
    pub fn new(
        pos: impl IntoIterator<Item = Ty>,
        named: impl IntoIterator<Item = (StrRef, Ty)>,
        rest: Option<Ty>,
        ret_ty: Option<Ty>,
    ) -> Self {
        let named = named
            .into_iter()
            .map(|(name, ty)| (name, ty, Span::detached()))
            .collect::<Vec<_>>();
        let (names, types) = RecordTy::shape_fields(named);
        let spread_right = rest.is_some();

        let name_started = if spread_right { 1 } else { 0 } + types.len();
        let types = pos.into_iter().chain(types).chain(rest).collect::<Vec<_>>();

        let name_started = (types.len() - name_started) as u32;

        Self {
            inputs: Interned::new(types),
            body: ret_ty,
            names: Interned::new(names),
            name_started,
            spread_left: false,
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

    fn field_by_bone_offset(&self, i: usize) -> Option<&Ty> {
        self.inputs.get(i + self.name_started as usize)
    }
}

impl SigTy {
    /// Get the input types of the function
    pub fn inputs(&self) -> impl Iterator<Item = &Ty> {
        self.inputs.iter()
    }

    /// Get the positional parameters of the function
    pub fn positional_params(&self) -> impl ExactSizeIterator<Item = &Ty> {
        self.inputs.iter().take(self.name_started as usize)
    }

    /// Get the parameter at the given index
    pub fn pos(&self, idx: usize) -> Option<&Ty> {
        (idx < self.name_started as usize)
            .then_some(())
            .and_then(|_| self.inputs.get(idx))
    }

    /// Get the named parameters of the function
    pub fn named_params(&self) -> impl ExactSizeIterator<Item = (&StrRef, &Ty)> {
        let named_names = self.names.names.iter();
        let named_types = self.inputs.iter().skip(self.name_started as usize);

        named_names.zip(named_types)
    }

    /// Get the named parameter by given name
    pub fn named(&self, name: &StrRef) -> Option<&Ty> {
        let idx = self.names.find(name)?;
        self.inputs.get(idx + self.name_started as usize)
    }

    /// Get the rest parameter of the function
    pub fn rest_param(&self) -> Option<&Ty> {
        if self.spread_right {
            self.inputs.last()
        } else {
            None
        }
    }

    /// Match the function type with the given arguments
    pub fn matches<'a>(
        &'a self,
        args: &'a SigTy,
        withs: Option<&'a Vec<Interned<crate::analysis::SigTy>>>,
    ) -> impl Iterator<Item = (&'a Ty, &'a Ty)> + 'a {
        let with_len = withs
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

        let arg_pos = withs
            .into_iter()
            .flat_map(|w| w.iter().rev().map(|w| w.positional_params()))
            .flatten()
            .chain(arg_pos);

        let sig_stream = sig_pos.chain(sig_rest.into_iter().cycle()).take(max_len);
        let arg_stream = arg_pos.chain(arg_rest.into_iter().cycle()).take(max_len);

        let pos = sig_stream.zip(arg_stream);
        let common_ifaces = withs
            .map(|e| e.iter().rev())
            .into_iter()
            .flatten()
            .flat_map(|w| self.common_iface_fields(w))
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

/// A function argument type
pub type ArgsTy = SigTy;

/// A type with partially applied arguments
#[derive(Hash, Clone, PartialEq, Eq)]
pub struct SigWithTy {
    /// The signature of the function
    pub sig: TyRef,
    /// The arguments applied to the function
    pub with: Interned<ArgsTy>,
}

impl SigWithTy {
    /// Create a type with applied arguments
    pub fn new(sig: TyRef, with: Interned<ArgsTy>) -> Interned<Self> {
        Interned::new(Self { sig, with })
    }
}

impl fmt::Debug for SigWithTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}.with({:?})", self.sig, self.with)
    }
}

/// A field selection type
#[derive(Hash, Clone, PartialEq, Eq)]
pub struct SelectTy {
    /// The type to select from
    pub ty: TyRef,
    /// The field to select
    pub select: StrRef,
}

impl SelectTy {
    /// Create a field selection type
    pub fn new(ty: TyRef, select: StrRef) -> Interned<Self> {
        Interned::new(Self { ty, select })
    }
}

impl fmt::Debug for SelectTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}.{}", RefDebug(&self.ty), self.select)
    }
}

/// The kind of unary operation
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub enum UnaryOp {
    /// The (arithmetic) positive operation
    /// `+t`
    Pos,
    /// The (arithmetic) negate operation
    /// `-t`
    Neg,
    /// The (logical) not operation
    /// `not t`
    Not,
    /// The typst context operation
    /// `context t`
    Context,
    /// The not element of operation
    /// `not in t`
    NotElementOf,
    /// The element of operation
    /// `in t`
    ElementOf,
    /// The type of operation
    /// `type(t)`
    TypeOf,
}

/// A unary operation type
#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct TypeUnary {
    /// The operand of the unary operation
    pub lhs: TyRef,
    /// The kind of the unary operation
    pub op: UnaryOp,
}

impl TypeUnary {
    /// Create a unary operation type
    pub fn new(op: UnaryOp, lhs: TyRef) -> Interned<Self> {
        Interned::new(Self { lhs, op })
    }

    /// Get the operands of the unary operation
    pub fn operands(&self) -> [&TyRef; 1] {
        [&self.lhs]
    }
}

/// The kind of binary operation
pub type BinaryOp = ast::BinOp;

/// A binary operation type
#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct TypeBinary {
    /// The operands of the binary operation
    pub operands: (TyRef, TyRef),
    /// The kind of the binary operation
    pub op: BinaryOp,
}

impl TypeBinary {
    /// Create a binary operation type
    pub fn new(op: BinaryOp, lhs: TyRef, rhs: TyRef) -> Interned<Self> {
        Interned::new(Self {
            operands: (lhs, rhs),
            op,
        })
    }

    /// Get the operands of the binary operation
    pub fn operands(&self) -> [&TyRef; 2] {
        [&self.operands.0, &self.operands.1]
    }
}

/// A conditional type
/// `if t1 then t2 else t3`
#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct IfTy {
    /// The condition
    pub cond: TyRef,
    /// The type when the condition is true
    pub then: TyRef,
    /// The type when the condition is false
    pub else_: TyRef,
}

impl IfTy {
    /// Create a conditional type
    pub fn new(cond: TyRef, then: TyRef, else_: TyRef) -> Interned<Self> {
        Interned::new(Self { cond, then, else_ })
    }
}

/// A type scheme on a group of syntax structures (typing)
#[derive(Default)]
pub struct TypeScheme {
    /// The typing on definitions
    pub vars: HashMap<DefId, TypeVarBounds>,
    /// The typing on syntax structures
    pub mapping: HashMap<Span, Vec<Ty>>,

    pub(super) cano_cache: Mutex<TypeCanoStore>,
}

impl TypeScheme {
    /// Get the type of a definition
    pub fn type_of_def(&self, def: DefId) -> Option<Ty> {
        Some(self.simplify(self.vars.get(&def).map(|e| e.as_type())?, false))
    }
    /// Get the type of a syntax structure
    pub fn type_of_span(&self, site: Span) -> Option<Ty> {
        self.mapping
            .get(&site)
            .cloned()
            .map(|e| Ty::from_types(e.into_iter()))
    }

    // todo: distinguish at least, at most
    /// Witness a lower-bound type on a syntax structure
    pub fn witness_at_least(&mut self, site: Span, ty: Ty) {
        Self::witness_(site, ty, &mut self.mapping);
    }
    /// Witness a upper-bound type on a syntax structure
    pub fn witness_at_most(&mut self, site: Span, ty: Ty) {
        Self::witness_(site, ty, &mut self.mapping);
    }

    /// Witness a type
    pub fn witness_(site: Span, ty: Ty, mapping: &mut HashMap<Span, Vec<Ty>>) {
        if site.is_detached() {
            return;
        }

        // todo: intersect/union
        let site_store = mapping.entry(site);
        match site_store {
            Entry::Occupied(e) => {
                e.into_mut().push(ty);
            }
            Entry::Vacant(e) => {
                e.insert(vec![ty]);
            }
        }
    }
}

/// A type variable bounds
#[derive(Clone)]
pub struct TypeVarBounds {
    /// The type variable representation
    pub var: Interned<TypeVar>,
    /// The bounds of the type variable
    pub bounds: FlowVarKind,
}

impl fmt::Debug for TypeVarBounds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.var)
    }
}

impl TypeVarBounds {
    /// Create a type variable bounds
    pub fn new(var: TypeVar, init: TypeBounds) -> Self {
        Self {
            var: Interned::new(var),
            bounds: FlowVarKind::Strong(Arc::new(RwLock::new(init))),
        }
    }

    /// Get the name of the type variable
    pub fn name(&self) -> StrRef {
        self.var.name.clone()
    }
    /// Get the definition id of the type variable
    pub fn id(&self) -> DefId {
        self.var.def
    }

    /// Get self as a type
    pub fn as_type(&self) -> Ty {
        Ty::Var(self.var.clone())
    }

    /// Slightly close the type variable
    pub fn weaken(&mut self) {
        match &self.bounds {
            FlowVarKind::Strong(w) => {
                self.bounds = FlowVarKind::Weak(w.clone());
            }
            FlowVarKind::Weak(_) => {}
        }
    }
}

/// A type variable bounds
#[derive(Clone)]
pub enum FlowVarKind {
    /// A type variable that receives both types and values (type instnaces)
    Strong(Arc<RwLock<TypeBounds>>),
    /// A type variable that receives only types
    /// The received values will be lifted to types
    Weak(Arc<RwLock<TypeBounds>>),
}

impl FlowVarKind {
    /// Get the bounds of the type variable
    pub fn bounds(&self) -> &RwLock<TypeBounds> {
        match self {
            FlowVarKind::Strong(w) | FlowVarKind::Weak(w) => w,
        }
    }
}

#[derive(Default)]
pub(super) struct TypeCanoStore {
    pub cano_cache: HashMap<(Ty, bool), Ty>,
    pub cano_local_cache: HashMap<(DefId, bool), Ty>,
    pub negatives: HashSet<DefId>,
    pub positives: HashSet<DefId>,
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

struct RefDebug<'a>(&'a Ty);

impl<'a> fmt::Debug for RefDebug<'a> {
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
            withs: Option<Vec<Interned<ArgsTy>>>,
        ) -> String {
            let res = sig.matches(&args, withs.as_ref()).collect::<Vec<_>>();
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
