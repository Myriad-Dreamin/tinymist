#![allow(unused)]

use core::fmt;
use ecow::EcoVec;
use once_cell::sync::OnceCell;
use parking_lot::{Mutex, RwLock};
use reflexo::vector::ir::DefId;
use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    hash::{Hash, Hasher},
    sync::Arc,
};
use typst::{
    foundations::Value,
    syntax::{ast, Span, SyntaxNode},
};

use crate::{
    adt::interner::{impl_internable, Interned},
    analysis::BuiltinTy,
};

pub type TyRef = Interned<Ty>;

#[derive(Default)]
pub(crate) struct TypeCheckInfo {
    pub vars: HashMap<DefId, TypeVarBounds>,
    pub mapping: HashMap<Span, Vec<Ty>>,

    pub(super) cano_cache: Mutex<TypeCanoStore>,
}

impl TypeCheckInfo {
    // todo: distinguish at least, at most
    pub fn witness_at_least(&mut self, site: Span, ty: Ty) {
        Self::witness_(site, ty, &mut self.mapping);
    }

    pub fn witness_at_most(&mut self, site: Span, ty: Ty) {
        Self::witness_(site, ty, &mut self.mapping);
    }

    pub(crate) fn witness_(site: Span, ty: Ty, mapping: &mut HashMap<Span, Vec<Ty>>) {
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

    pub fn type_of_span(&self, site: Span) -> Option<Ty> {
        self.mapping
            .get(&site)
            .cloned()
            .map(|e| Ty::from_types(e.into_iter()))
    }

    pub fn type_of_def(&self, def: DefId) -> Option<Ty> {
        Some(self.simplify(self.vars.get(&def).map(|e| e.as_type())?, false))
    }
}

#[derive(Default)]
pub(super) struct TypeCanoStore {
    pub cano_cache: HashMap<(Ty, bool), Ty>,
    pub cano_local_cache: HashMap<(DefId, bool), Ty>,
    pub negatives: HashSet<DefId>,
    pub positives: HashSet<DefId>,
}

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
                    return name.into();
                }
                let name = self.name_node.clone().into_text();
                name.into()
            })
            .clone()
    }
}

pub trait TypeInterace {
    fn bone(&self) -> &Interned<NameBone>;
    fn interface(&self) -> impl Iterator<Item = (&Interned<str>, &Ty)>;
    fn field_by_bone_offset(&self, i: usize) -> Option<&Ty>;

    fn common_iface_fields<'a>(
        &'a self,
        rhs: &'a Self,
    ) -> impl Iterator<Item = (&'a Interned<str>, &'a Ty, &'a Ty)> {
        let lhs_names = self.bone();
        let rhs_names = rhs.bone();

        lhs_names
            .intersect_keys_enumerate(rhs_names)
            .filter_map(move |(i, j)| {
                let lhs = self.field_by_bone_offset(i)?;
                let rhs = rhs.field_by_bone_offset(j)?;
                Some((&lhs_names.names[i], lhs, rhs))
            })
    }
}

struct RefDebug<'a>(&'a Ty);

impl<'a> fmt::Debug for RefDebug<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Ty::Var(v) => write!(f, "@v{:?}", v.name()),
            _ => write!(f, "{:?}", self.0),
        }
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
        Self { val, syntax: None }.into()
    }
    pub fn new_at(val: Value, s: Span) -> Interned<Self> {
        Interned::new(Self {
            val,
            syntax: Some(Interned::new(TypeSource {
                name_node: SyntaxNode::default(),
                name_repr: OnceCell::new(),
                span: s,
                doc: "".into(),
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
                doc: doc.into(),
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

impl NameBone {
    pub(crate) fn intersect_keys_enumerate<'a>(
        &'a self,
        rhs: &'a NameBone,
    ) -> impl Iterator<Item = (usize, usize)> + 'a {
        let mut lhs_iter = self.names.iter().enumerate();
        let mut rhs_iter = rhs.names.iter().enumerate();

        let mut lhs = lhs_iter.next();
        let mut rhs = rhs_iter.next();

        std::iter::from_fn(move || 'key_scanning: loop {
            if let (Some((i, lhs_key)), Some((j, rhs_key))) = (lhs, rhs) {
                match lhs_key.cmp(rhs_key) {
                    std::cmp::Ordering::Less => {
                        lhs = lhs_iter.next();
                        continue 'key_scanning;
                    }
                    std::cmp::Ordering::Greater => {
                        rhs = rhs_iter.next();
                        continue 'key_scanning;
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

#[derive(Hash, Clone, PartialEq, Eq, Default)]
pub struct TypeBounds {
    pub lbs: EcoVec<Ty>,
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
#[derive(Hash, Clone, PartialEq, Eq)]
pub struct TypeVar {
    pub name: Interned<str>,
    pub def: DefId,

    pub syntax: Option<Interned<TypeSource>>,
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

#[derive(Clone)]
pub(crate) enum FlowVarKind {
    Strong(Arc<RwLock<TypeBounds>>),
    Weak(Arc<RwLock<TypeBounds>>),
}

impl FlowVarKind {
    pub fn bounds(&self) -> &RwLock<TypeBounds> {
        match self {
            FlowVarKind::Strong(w) | FlowVarKind::Weak(w) => w,
        }
    }
}

#[derive(Clone)]
pub struct TypeVarBounds {
    pub var: Interned<TypeVar>,
    pub bounds: FlowVarKind,
}

impl fmt::Debug for TypeVarBounds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self.var)
    }
}

impl TypeVarBounds {
    pub fn name(&self) -> Interned<str> {
        self.var.name.clone()
    }

    pub fn id(&self) -> DefId {
        self.var.def
    }

    pub fn as_type(&self) -> Ty {
        Ty::Var(self.var.clone())
    }

    pub(crate) fn new(var: TypeVar, init: TypeBounds) -> Self {
        Self {
            var: Interned::new(var),
            bounds: FlowVarKind::Strong(Arc::new(RwLock::new(init))),
        }
    }

    pub(crate) fn weaken(&mut self) {
        match &self.bounds {
            FlowVarKind::Strong(w) => {
                self.bounds = FlowVarKind::Weak(w.clone());
            }
            FlowVarKind::Weak(_) => {}
        }
    }
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

#[derive(Hash, Clone, PartialEq, Eq)]
pub struct RecordTy {
    pub types: Interned<Vec<Ty>>,
    pub names: Interned<NameBone>,
    pub syntax: Option<Interned<TypeSource>>,
}

impl RecordTy {
    pub(crate) fn shape_fields(mut fields: Vec<(Interned<str>, Ty, Span)>) -> (NameBone, Vec<Ty>) {
        fields.sort_by(|a, b| a.0.cmp(&b.0));
        let names = NameBone {
            names: fields.iter().map(|e| e.0.clone()).collect(),
        };
        let types = fields.into_iter().map(|(_, ty, _)| ty).collect::<Vec<_>>();

        (names, types)
    }

    pub(crate) fn new(fields: Vec<(Interned<str>, Ty, Span)>) -> Interned<Self> {
        let (names, types) = Self::shape_fields(fields);
        Interned::new(Self {
            types: Interned::new(types),
            names: Interned::new(names),
            syntax: None,
        })
    }

    pub(crate) fn intersect_keys<'a>(
        &'a self,
        rhs: &'a RecordTy,
    ) -> impl Iterator<Item = (&Interned<str>, &Ty, &Ty)> + 'a {
        self.names
            .intersect_keys_enumerate(&rhs.names)
            .filter_map(move |(i, j)| {
                self.types
                    .get(i)
                    .and_then(|lhs| rhs.types.get(j).map(|rhs| (&self.names.names[i], lhs, rhs)))
            })
    }
}

impl TypeInterace for RecordTy {
    fn bone(&self) -> &Interned<NameBone> {
        &self.names
    }

    fn field_by_bone_offset(&self, i: usize) -> Option<&Ty> {
        self.types.get(i)
    }

    fn interface(&self) -> impl Iterator<Item = (&Interned<str>, &Ty)> {
        self.names.names.iter().zip(self.types.iter())
    }
}

impl fmt::Debug for RecordTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("{")?;
        interpersed(
            f,
            self.interface().map(|(name, ty)| ParamTy::Named(name, ty)),
        )?;
        f.write_str("}")
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
            ParamTy::Pos(ty) => write!(f, "{ty:?}"),
            ParamTy::Named(name, ty) => write!(f, "{name:?}: {ty:?}"),
            ParamTy::Rest(ty) => write!(f, "...: {ty:?}[]"),
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

    pub(crate) fn inputs(&self) -> impl Iterator<Item = &Ty> {
        self.types.iter()
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
        pos: impl IntoIterator<Item = Ty>,
        named: impl IntoIterator<Item = (Interned<str>, Ty)>,
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

impl TypeInterace for SigTy {
    fn bone(&self) -> &Interned<NameBone> {
        &self.names
    }

    fn interface(&self) -> impl Iterator<Item = (&Interned<str>, &Ty)> {
        let names = self.names.names.iter();
        let types = self.types.iter().skip(self.name_started as usize);
        names.zip(types)
    }

    fn field_by_bone_offset(&self, i: usize) -> Option<&Ty> {
        self.types.get(i + self.name_started as usize)
    }
}

impl SigTy {
    pub fn positional_params(&self) -> impl ExactSizeIterator<Item = &Ty> {
        self.types.iter().take(self.name_started as usize)
    }

    pub fn named_params(&self) -> impl ExactSizeIterator<Item = (&Interned<str>, &Ty)> {
        let named_names = self.names.names.iter();
        let named_types = self.types.iter().skip(self.name_started as usize);

        named_names.zip(named_types)
    }

    pub fn rest_param(&self) -> Option<&Ty> {
        if self.spread_right {
            self.types.last()
        } else {
            None
        }
    }

    pub fn pos(&self, idx: usize) -> Option<&Ty> {
        (idx < self.name_started as usize)
            .then_some(())
            .and_then(|_| self.types.get(idx))
    }

    pub fn named(&self, name: &Interned<str>) -> Option<&Ty> {
        let idx = self.names.find(name)?;
        self.types.get(idx + self.name_started as usize)
    }

    pub(crate) fn matches<'a>(
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

        let mut pos = sig_stream.zip(arg_stream);
        let common_ifaces = withs
            .map(|e| e.iter().rev())
            .into_iter()
            .flatten()
            .flat_map(|w| self.common_iface_fields(w))
            .chain(self.common_iface_fields(args));
        let mut named = common_ifaces.map(|(n, l, r)| (l, r));

        pos.chain(named)
    }
}

impl fmt::Debug for SigTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("(")?;
        let pos = self.positional_params().map(ParamTy::Pos);
        let named = self
            .named_params()
            .map(|(name, ty)| ParamTy::Named(name, ty));
        let rest = self.rest_param().map(ParamTy::Rest);
        interpersed(f, pos.chain(named).chain(rest))?;
        f.write_str(") => ")?;
        if let Some(ret) = &self.ret {
            ret.fmt(f)?;
        } else {
            f.write_str("any")?;
        }
        Ok(())
    }
}

pub type ArgsTy = SigTy;

#[derive(Hash, Clone, PartialEq, Eq)]
pub struct SigWithTy {
    pub sig: TyRef,
    pub with: Interned<ArgsTy>,
}

impl SigWithTy {
    pub fn new(sig: TyRef, with: Interned<ArgsTy>) -> Interned<Self> {
        Interned::new(Self { sig, with })
    }
}

impl fmt::Debug for SigWithTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}.with({:?})", self.sig, self.with)
    }
}

#[derive(Hash, Clone, PartialEq, Eq)]
pub struct SelectTy {
    pub ty: Interned<Ty>,
    pub select: Interned<str>,
}

impl SelectTy {
    pub fn new(ty: Interned<Ty>, select: Interned<str>) -> Interned<Self> {
        Interned::new(Self { ty, select })
    }
}

impl fmt::Debug for SelectTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}.{}", RefDebug(&self.ty), self.select)
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub(crate) enum UnaryOp {
    Pos,
    Neg,
    Not,
    Context,
    NotElementOf,
    ElementOf,
    TypeOf,
}

#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct TypeUnary {
    pub lhs: Interned<Ty>,
    pub op: UnaryOp,
}

impl TypeUnary {
    pub fn new(op: UnaryOp, lhs: Interned<Ty>) -> Interned<Self> {
        Interned::new(Self { lhs, op })
    }
}

#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub struct TypeBinary {
    pub operands: (Interned<Ty>, Interned<Ty>),
    pub op: ast::BinOp,
}

impl TypeBinary {
    pub fn new(op: ast::BinOp, lhs: Interned<Ty>, rhs: Interned<Ty>) -> Interned<Self> {
        Interned::new(Self {
            operands: (lhs, rhs),
            op,
        })
    }

    pub fn repr(&self) -> (&Interned<Ty>, &Interned<Ty>) {
        (&self.operands.0, &self.operands.1)
    }
}

#[derive(Debug, Hash, Clone, PartialEq, Eq)]
pub(crate) struct IfTy {
    pub cond: Interned<Ty>,
    pub then: Interned<Ty>,
    pub else_: Interned<Ty>,
}

impl IfTy {
    pub fn new(cond: Interned<Ty>, then: Interned<Ty>, else_: Interned<Ty>) -> Interned<Self> {
        Interned::new(Self { cond, then, else_ })
    }
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
    pub(crate) fn is_dict(&self) -> bool {
        matches!(self, Ty::Dict(..))
    }

    pub(crate) fn from_types(e: impl ExactSizeIterator<Item = Ty>) -> Self {
        if e.len() == 0 {
            Ty::Any
        } else if e.len() == 1 {
            let mut e = e;
            e.next().unwrap()
        } else {
            Self::iter_union(e)
        }
    }

    pub(crate) fn iter_union(e: impl IntoIterator<Item = Ty>) -> Self {
        Ty::Union(Interned::new(e.into_iter().collect()))
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
    use insta::{assert_debug_snapshot, assert_snapshot};

    use crate::ty::tests::*;
    #[test]
    fn test_ty() {
        use super::*;
        let ty = Ty::Clause;
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
