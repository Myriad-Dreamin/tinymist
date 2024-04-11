//! Top-level evaluation of a source file.

use core::fmt;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use ecow::EcoString;
use parking_lot::{Mutex, RwLock};
use reflexo::{hash::hash128, vector::ir::DefId};
use typst::{
    foundations::{CastInfo, Element, Func, ParamInfo, Value},
    syntax::{
        ast::{self, AstNode},
        LinkedNode, Source, Span, SyntaxKind,
    },
};

use crate::{analysis::analyze_dyn_signature, AnalysisContext};

use super::{resolve_global_value, DefUseInfo, IdentRef};

struct RefDebug<'a>(&'a FlowType);

impl<'a> fmt::Debug for RefDebug<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            FlowType::Var(v) => write!(f, "@{}", v.1),
            _ => write!(f, "{:?}", self.0),
        }
    }
}
#[derive(Debug, Clone, Hash)]
pub(crate) enum FlowUnaryType {
    Pos(Box<FlowType>),
    Neg(Box<FlowType>),
    Not(Box<FlowType>),
}

impl FlowUnaryType {
    pub fn lhs(&self) -> &FlowType {
        match self {
            FlowUnaryType::Pos(e) => e,
            FlowUnaryType::Neg(e) => e,
            FlowUnaryType::Not(e) => e,
        }
    }
}

#[derive(Debug, Clone, Hash)]
pub(crate) enum FlowBinaryType {
    Add(FlowBinaryRepr),
    Sub(FlowBinaryRepr),
    Mul(FlowBinaryRepr),
    Div(FlowBinaryRepr),
    And(FlowBinaryRepr),
    Or(FlowBinaryRepr),
    Eq(FlowBinaryRepr),
    Neq(FlowBinaryRepr),
    Lt(FlowBinaryRepr),
    Leq(FlowBinaryRepr),
    Gt(FlowBinaryRepr),
    Geq(FlowBinaryRepr),
    Assign(FlowBinaryRepr),
    In(FlowBinaryRepr),
    NotIn(FlowBinaryRepr),
    AddAssign(FlowBinaryRepr),
    SubAssign(FlowBinaryRepr),
    MulAssign(FlowBinaryRepr),
    DivAssign(FlowBinaryRepr),
}

impl FlowBinaryType {
    pub fn repr(&self) -> &FlowBinaryRepr {
        match self {
            FlowBinaryType::Add(r)
            | FlowBinaryType::Sub(r)
            | FlowBinaryType::Mul(r)
            | FlowBinaryType::Div(r)
            | FlowBinaryType::And(r)
            | FlowBinaryType::Or(r)
            | FlowBinaryType::Eq(r)
            | FlowBinaryType::Neq(r)
            | FlowBinaryType::Lt(r)
            | FlowBinaryType::Leq(r)
            | FlowBinaryType::Gt(r)
            | FlowBinaryType::Geq(r)
            | FlowBinaryType::Assign(r)
            | FlowBinaryType::In(r)
            | FlowBinaryType::NotIn(r)
            | FlowBinaryType::AddAssign(r)
            | FlowBinaryType::SubAssign(r)
            | FlowBinaryType::MulAssign(r)
            | FlowBinaryType::DivAssign(r) => r,
        }
    }
}

#[derive(Clone, Hash)]
pub(crate) struct FlowBinaryRepr(Box<(FlowType, FlowType)>);

impl fmt::Debug for FlowBinaryRepr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // shorter
        write!(f, "{:?}, {:?}", RefDebug(&self.0 .0), RefDebug(&self.0 .1))
    }
}

#[derive(Clone, Hash)]
pub(crate) struct FlowVarStore {
    pub lbs: Vec<FlowType>,
    pub ubs: Vec<FlowType>,
}

impl fmt::Debug for FlowVarStore {
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

#[derive(Clone)]
pub(crate) enum FlowVarKind {
    Weak(Arc<RwLock<FlowVarStore>>),
}

#[derive(Clone)]
pub(crate) struct FlowVar {
    pub name: EcoString,
    pub id: DefId,
    pub kind: FlowVarKind,
}

impl std::hash::Hash for FlowVar {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        0.hash(state);
        self.id.hash(state);
    }
}

impl fmt::Debug for FlowVar {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{}", self.name)?;
        match &self.kind {
            // FlowVarKind::Strong(t) => write!(f, " = {:?}", t),
            FlowVarKind::Weak(w) => write!(f, "{w:?}"),
        }
    }
}

impl FlowVar {
    pub fn name(&self) -> EcoString {
        self.name.clone()
    }

    pub fn id(&self) -> DefId {
        self.id
    }

    pub fn get_ref(&self) -> FlowType {
        FlowType::Var(Box::new((self.id, self.name.clone())))
    }

    fn ever_be(&self, exp: FlowType) {
        match &self.kind {
            // FlowVarKind::Strong(_t) => {}
            FlowVarKind::Weak(w) => {
                let mut w = w.write();
                w.lbs.push(exp.clone());
            }
        }
    }

    fn as_strong(&mut self, exp: FlowType) {
        // self.kind = FlowVarKind::Strong(value);
        match &self.kind {
            // FlowVarKind::Strong(_t) => {}
            FlowVarKind::Weak(w) => {
                let mut w = w.write();
                w.lbs.push(exp.clone());
            }
        }
    }
}

#[derive(Hash, Clone)]
pub(crate) struct FlowAt(Box<(FlowType, EcoString)>);

impl fmt::Debug for FlowAt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}.{}", RefDebug(&self.0 .0), self.0 .1)
    }
}

#[derive(Clone, Hash)]
pub(crate) struct FlowArgs {
    pub args: Vec<FlowType>,
    pub named: Vec<(EcoString, FlowType)>,
}
impl FlowArgs {
    fn start_match(&self) -> &[FlowType] {
        &self.args
    }
}

impl fmt::Debug for FlowArgs {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use std::fmt::Write;

        f.write_str("&(")?;
        if let Some((first, args)) = self.args.split_first() {
            write!(f, "{first:?}")?;
            for arg in args {
                write!(f, "{arg:?}, ")?;
            }
        }
        f.write_char(')')
    }
}

#[derive(Clone, Hash)]
pub(crate) struct FlowSignature {
    pub pos: Vec<FlowType>,
    pub named: Vec<(EcoString, FlowType)>,
    pub rest: Option<FlowType>,
    pub ret: FlowType,
}

impl fmt::Debug for FlowSignature {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("(")?;
        if let Some((first, pos)) = self.pos.split_first() {
            write!(f, "{first:?}")?;
            for p in pos {
                write!(f, ", {p:?}")?;
            }
        }
        for (name, ty) in &self.named {
            write!(f, ", {name}: {ty:?}")?;
        }
        if let Some(rest) = &self.rest {
            write!(f, ", ...: {rest:?}")?;
        }
        f.write_str(") -> ")?;
        write!(f, "{:?}", self.ret)
    }
}

#[derive(Debug, Clone, Hash)]
pub(crate) enum PathPreference {
    None,
    Source,
    Image,
    Json,
    Yaml,
    Xml,
    Toml,
}

impl PathPreference {
    pub(crate) fn match_ext(&self, ext: &std::ffi::OsStr) -> bool {
        let ext = || ext.to_str().map(|e| e.to_lowercase()).unwrap_or_default();

        match self {
            PathPreference::None => true,
            PathPreference::Source => {
                matches!(ext().as_ref(), "typ")
            }
            PathPreference::Image => {
                matches!(
                    ext().as_ref(),
                    "png" | "webp" | "jpg" | "jpeg" | "svg" | "svgz"
                )
            }
            PathPreference::Json => {
                matches!(ext().as_ref(), "json" | "jsonc" | "json5")
            }
            PathPreference::Yaml => matches!(ext().as_ref(), "yaml" | "yml"),
            PathPreference::Xml => matches!(ext().as_ref(), "xml"),
            PathPreference::Toml => matches!(ext().as_ref(), "toml"),
        }
    }
}

#[derive(Debug, Clone, Hash)]
pub(crate) enum FlowBuiltinType {
    Args,
    Path(PathPreference),
}

#[derive(Hash, Clone)]
#[allow(clippy::box_collection)]
pub(crate) enum FlowType {
    Clause,
    Undef,
    Content,
    Any,
    Array,
    Dict,
    None,
    Infer,
    FlowNone,
    Auto,
    Builtin(FlowBuiltinType),

    Args(Box<FlowArgs>),
    Func(Box<FlowSignature>),
    With(Box<(FlowType, Vec<FlowArgs>)>),
    At(FlowAt),
    Union(Box<Vec<FlowType>>),
    Let(Arc<FlowVarStore>),
    Var(Box<(DefId, EcoString)>),
    Unary(FlowUnaryType),
    Binary(FlowBinaryType),
    Value(Box<Value>),
    ValueDoc(Box<(Value, &'static str)>),
    Element(Element),
}

impl fmt::Debug for FlowType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FlowType::Clause => f.write_str("Clause"),
            FlowType::Undef => f.write_str("Undef"),
            FlowType::Content => f.write_str("Content"),
            FlowType::Any => f.write_str("Any"),
            FlowType::Array => f.write_str("Array"),
            FlowType::Dict => f.write_str("Dict"),
            FlowType::None => f.write_str("None"),
            FlowType::Infer => f.write_str("Infer"),
            FlowType::FlowNone => f.write_str("FlowNone"),
            FlowType::Auto => f.write_str("Auto"),
            FlowType::Builtin(t) => write!(f, "{t:?}"),
            FlowType::Args(a) => write!(f, "&({a:?})"),
            FlowType::Func(s) => write!(f, "{s:?}"),
            FlowType::With(w) => write!(f, "({:?}).with(..{:?})", w.0, w.1),
            FlowType::At(a) => write!(f, "{a:?}"),
            FlowType::Union(u) => {
                f.write_str("(")?;
                if let Some((first, u)) = u.split_first() {
                    write!(f, "{first:?}")?;
                    for u in u {
                        write!(f, " | {u:?}")?;
                    }
                }
                f.write_str(")")
            }
            FlowType::Let(v) => write!(f, "{v:?}"),
            FlowType::Var(v) => write!(f, "@{}", v.1),
            FlowType::Unary(u) => write!(f, "{u:?}"),
            FlowType::Binary(b) => write!(f, "{b:?}"),
            FlowType::Value(v) => write!(f, "{v:?}"),
            FlowType::ValueDoc(v) => write!(f, "{v:?}"),
            FlowType::Element(e) => write!(f, "{e:?}"),
        }
    }
}

impl FlowType {
    pub fn from_return_site(f: &Func, c: &'_ CastInfo) -> Option<Self> {
        use typst::foundations::func::Repr;
        match f.inner() {
            Repr::Element(e) => return Some(FlowType::Element(*e)),
            Repr::Closure(_) => {}
            Repr::With(w) => return FlowType::from_return_site(&w.0, c),
            Repr::Native(_) => {}
        };

        let ty = match c {
            CastInfo::Any => FlowType::Any,
            CastInfo::Value(v, doc) => FlowType::ValueDoc(Box::new((v.clone(), *doc))),
            CastInfo::Type(ty) => FlowType::Value(Box::new(Value::Type(*ty))),
            CastInfo::Union(e) => FlowType::Union(Box::new(
                e.iter()
                    .flat_map(|e| Self::from_return_site(f, e))
                    .collect(),
            )),
        };

        Some(ty)
    }

    pub(crate) fn from_param_site(f: &Func, p: &ParamInfo, s: &CastInfo) -> Option<FlowType> {
        use typst::foundations::func::Repr;
        match f.inner() {
            Repr::Element(..) | Repr::Native(..) => match (f.name().unwrap(), p.name) {
                ("image", "path") => {
                    return Some(FlowType::Builtin(FlowBuiltinType::Path(
                        PathPreference::Image,
                    )))
                }
                ("read", "path") => {
                    return Some(FlowType::Builtin(FlowBuiltinType::Path(
                        PathPreference::None,
                    )))
                }
                ("json", "path") => {
                    return Some(FlowType::Builtin(FlowBuiltinType::Path(
                        PathPreference::Json,
                    )))
                }
                ("yaml", "path") => {
                    return Some(FlowType::Builtin(FlowBuiltinType::Path(
                        PathPreference::Yaml,
                    )))
                }
                ("xml", "path") => {
                    return Some(FlowType::Builtin(FlowBuiltinType::Path(
                        PathPreference::Xml,
                    )))
                }
                ("toml", "path") => {
                    return Some(FlowType::Builtin(FlowBuiltinType::Path(
                        PathPreference::Toml,
                    )))
                }
                _ => {}
            },
            Repr::Closure(_) => {}
            Repr::With(w) => return FlowType::from_param_site(&w.0, p, s),
        };

        let ty = match &s {
            CastInfo::Any => FlowType::Any,
            CastInfo::Value(v, doc) => FlowType::ValueDoc(Box::new((v.clone(), *doc))),
            CastInfo::Type(ty) => FlowType::Value(Box::new(Value::Type(*ty))),
            CastInfo::Union(e) => FlowType::Union(Box::new(
                e.iter()
                    .flat_map(|e| Self::from_param_site(f, p, e))
                    .collect(),
            )),
        };

        Some(ty)
    }
}

pub(crate) struct TypeCheckInfo {
    pub vars: HashMap<DefId, FlowVar>,
    pub mapping: HashMap<Span, FlowType>,

    cano_cache: Mutex<TypeCanoStore>,
}

impl TypeCheckInfo {
    pub fn simplify(&self, ty: FlowType) -> FlowType {
        let mut c = self.cano_cache.lock();
        let c = &mut *c;

        c.cano_local_cache.clear();
        c.positives.clear();
        c.negatives.clear();

        let mut worker = TypeSimplifier {
            vars: &self.vars,
            cano_cache: &mut c.cano_cache,
            cano_local_cache: &mut c.cano_local_cache,

            positives: &mut c.positives,
            negatives: &mut c.negatives,
        };

        worker.simplify(ty)
    }
}

pub(crate) fn type_check(ctx: &mut AnalysisContext, source: Source) -> Option<Arc<TypeCheckInfo>> {
    let def_use_info = ctx.def_use(source.clone())?;
    let mut info = TypeCheckInfo {
        vars: HashMap::new(),
        mapping: HashMap::new(),

        cano_cache: Mutex::new(TypeCanoStore::default()),
    };
    let mut type_checker = TypeChecker {
        ctx,
        source: source.clone(),
        def_use_info,
        info: &mut info,
        mode: InterpretMode::Markup,
    };
    let lnk = LinkedNode::new(source.root());

    let current = std::time::Instant::now();
    type_checker.check(lnk);
    let elapsed = current.elapsed();
    log::info!("Type checking on {:?} took {:?}", source.id(), elapsed);

    let _ = type_checker.info.mapping;
    let _ = type_checker.source;

    Some(Arc::new(info))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InterpretMode {
    Markup,
    Code,
    Math,
}

struct TypeChecker<'a, 'w> {
    ctx: &'a mut AnalysisContext<'w>,
    source: Source,
    def_use_info: Arc<DefUseInfo>,

    info: &'a mut TypeCheckInfo,
    mode: InterpretMode,
}

impl<'a, 'w> TypeChecker<'a, 'w> {
    fn check(&mut self, root: LinkedNode) -> FlowType {
        let should_record = matches!(root.kind(), SyntaxKind::FuncCall).then(|| root.span());
        let w = self.check_inner(root).unwrap_or(FlowType::Undef);

        if let Some(s) = should_record {
            self.info.mapping.insert(s, w.clone());
        }

        w
    }

    fn check_inner(&mut self, root: LinkedNode) -> Option<FlowType> {
        Some(match root.kind() {
            SyntaxKind::Markup => return self.check_in_mode(root, InterpretMode::Markup),
            SyntaxKind::Math => return self.check_in_mode(root, InterpretMode::Math),
            SyntaxKind::Code => return self.check_in_mode(root, InterpretMode::Code),
            SyntaxKind::CodeBlock => return self.check_in_mode(root, InterpretMode::Code),
            SyntaxKind::ContentBlock => return self.check_in_mode(root, InterpretMode::Markup),

            SyntaxKind::Text => FlowType::Content,
            SyntaxKind::Space => FlowType::Content,
            SyntaxKind::Linebreak => FlowType::Content,
            SyntaxKind::Parbreak => FlowType::Content,
            SyntaxKind::Escape => FlowType::Content,
            SyntaxKind::Shorthand => FlowType::Content,
            SyntaxKind::SmartQuote => FlowType::Content,
            SyntaxKind::Raw => FlowType::Content,
            SyntaxKind::RawLang => FlowType::Content,
            SyntaxKind::RawDelim => FlowType::Content,
            SyntaxKind::RawTrimmed => FlowType::Content,
            SyntaxKind::Link => FlowType::Content,
            SyntaxKind::Label => FlowType::Content,
            SyntaxKind::Ref => FlowType::Content,
            SyntaxKind::RefMarker => FlowType::Content,
            SyntaxKind::HeadingMarker => FlowType::Content,
            SyntaxKind::EnumMarker => FlowType::Content,
            SyntaxKind::ListMarker => FlowType::Content,
            SyntaxKind::TermMarker => FlowType::Content,
            SyntaxKind::MathAlignPoint => FlowType::Content,
            SyntaxKind::MathPrimes => FlowType::Content,

            SyntaxKind::Strong => return self.check_children(root),
            SyntaxKind::Emph => return self.check_children(root),
            SyntaxKind::Heading => return self.check_children(root),
            SyntaxKind::ListItem => return self.check_children(root),
            SyntaxKind::EnumItem => return self.check_children(root),
            SyntaxKind::TermItem => return self.check_children(root),
            SyntaxKind::Equation => return self.check_children(root),
            SyntaxKind::MathDelimited => return self.check_children(root),
            SyntaxKind::MathAttach => return self.check_children(root),
            SyntaxKind::MathFrac => return self.check_children(root),
            SyntaxKind::MathRoot => return self.check_children(root),

            SyntaxKind::LoopBreak => FlowType::None,
            SyntaxKind::LoopContinue => FlowType::None,
            SyntaxKind::FuncReturn => FlowType::None,
            SyntaxKind::LineComment => FlowType::None,
            SyntaxKind::BlockComment => FlowType::None,
            SyntaxKind::Error => FlowType::None,
            SyntaxKind::Eof => FlowType::None,

            SyntaxKind::None => FlowType::None,
            SyntaxKind::Auto => FlowType::Auto,
            SyntaxKind::Break => FlowType::FlowNone,
            SyntaxKind::Continue => FlowType::FlowNone,
            SyntaxKind::Return => FlowType::FlowNone,
            SyntaxKind::Ident => return self.check_ident(root, InterpretMode::Code),
            SyntaxKind::MathIdent => return self.check_ident(root, InterpretMode::Math),
            SyntaxKind::Bool
            | SyntaxKind::Int
            | SyntaxKind::Float
            | SyntaxKind::Numeric
            | SyntaxKind::Str => {
                return self
                    .ctx
                    .mini_eval(root.cast()?)
                    .map(Box::new)
                    .map(FlowType::Value)
            }
            SyntaxKind::Parenthesized => return self.check_children(root),
            SyntaxKind::Array => return self.check_array(root),
            SyntaxKind::Dict => return self.check_dict(root),
            SyntaxKind::Unary => return self.check_unary(root),
            SyntaxKind::Binary => return self.check_binary(root),
            SyntaxKind::FieldAccess => return self.check_field_access(root),
            SyntaxKind::FuncCall => return self.check_func_call(root),
            SyntaxKind::Args => return self.check_args(root),
            SyntaxKind::Closure => return self.check_closure(root),
            SyntaxKind::LetBinding => return self.check_let(root),
            SyntaxKind::SetRule => return self.check_set(root),
            SyntaxKind::ShowRule => return self.check_show(root),
            SyntaxKind::Contextual => return self.check_contextual(root),
            SyntaxKind::Conditional => return self.check_conditional(root),
            SyntaxKind::WhileLoop => return self.check_while_loop(root),
            SyntaxKind::ForLoop => return self.check_for_loop(root),
            SyntaxKind::ModuleImport => return self.check_module_import(root),
            SyntaxKind::ModuleInclude => return self.check_module_include(root),
            SyntaxKind::Destructuring => return self.check_destructuring(root),
            SyntaxKind::DestructAssignment => return self.check_destruct_assign(root),

            // Rest all are clauses
            SyntaxKind::Named => FlowType::Clause,
            SyntaxKind::Keyed => FlowType::Clause,
            SyntaxKind::Spread => FlowType::Clause,
            SyntaxKind::Params => FlowType::Clause,
            SyntaxKind::ImportItems => FlowType::Clause,
            SyntaxKind::RenamedImportItem => FlowType::Clause,
            SyntaxKind::Hash => FlowType::Clause,
            SyntaxKind::LeftBrace => FlowType::Clause,
            SyntaxKind::RightBrace => FlowType::Clause,
            SyntaxKind::LeftBracket => FlowType::Clause,
            SyntaxKind::RightBracket => FlowType::Clause,
            SyntaxKind::LeftParen => FlowType::Clause,
            SyntaxKind::RightParen => FlowType::Clause,
            SyntaxKind::Comma => FlowType::Clause,
            SyntaxKind::Semicolon => FlowType::Clause,
            SyntaxKind::Colon => FlowType::Clause,
            SyntaxKind::Star => FlowType::Clause,
            SyntaxKind::Underscore => FlowType::Clause,
            SyntaxKind::Dollar => FlowType::Clause,
            SyntaxKind::Plus => FlowType::Clause,
            SyntaxKind::Minus => FlowType::Clause,
            SyntaxKind::Slash => FlowType::Clause,
            SyntaxKind::Hat => FlowType::Clause,
            SyntaxKind::Prime => FlowType::Clause,
            SyntaxKind::Dot => FlowType::Clause,
            SyntaxKind::Eq => FlowType::Clause,
            SyntaxKind::EqEq => FlowType::Clause,
            SyntaxKind::ExclEq => FlowType::Clause,
            SyntaxKind::Lt => FlowType::Clause,
            SyntaxKind::LtEq => FlowType::Clause,
            SyntaxKind::Gt => FlowType::Clause,
            SyntaxKind::GtEq => FlowType::Clause,
            SyntaxKind::PlusEq => FlowType::Clause,
            SyntaxKind::HyphEq => FlowType::Clause,
            SyntaxKind::StarEq => FlowType::Clause,
            SyntaxKind::SlashEq => FlowType::Clause,
            SyntaxKind::Dots => FlowType::Clause,
            SyntaxKind::Arrow => FlowType::Clause,
            SyntaxKind::Root => FlowType::Clause,
            SyntaxKind::Not => FlowType::Clause,
            SyntaxKind::And => FlowType::Clause,
            SyntaxKind::Or => FlowType::Clause,
            SyntaxKind::Let => FlowType::Clause,
            SyntaxKind::Set => FlowType::Clause,
            SyntaxKind::Show => FlowType::Clause,
            SyntaxKind::Context => FlowType::Clause,
            SyntaxKind::If => FlowType::Clause,
            SyntaxKind::Else => FlowType::Clause,
            SyntaxKind::For => FlowType::Clause,
            SyntaxKind::In => FlowType::Clause,
            SyntaxKind::While => FlowType::Clause,
            SyntaxKind::Import => FlowType::Clause,
            SyntaxKind::Include => FlowType::Clause,
            SyntaxKind::As => FlowType::Clause,
        })
    }

    fn check_in_mode(&mut self, root: LinkedNode, into_mode: InterpretMode) -> Option<FlowType> {
        let mode = self.mode;
        self.mode = into_mode;
        let res = self.check_children(root);
        self.mode = mode;
        res
    }

    fn check_children(&mut self, root: LinkedNode<'_>) -> Option<FlowType> {
        for child in root.children() {
            self.check(child);
        }
        Some(FlowType::Content)
    }

    fn check_ident(&mut self, root: LinkedNode<'_>, mode: InterpretMode) -> Option<FlowType> {
        let ident: ast::Ident = root.cast()?;
        let ident_ref = IdentRef {
            name: ident.get().to_string(),
            range: root.range(),
        };

        let Some(def_id) = self.def_use_info.get_ref(&ident_ref) else {
            let v = resolve_global_value(self.ctx, root, mode == InterpretMode::Math)?;
            return Some(FlowType::Value(Box::new(v)));
        };
        let var = self.info.vars.get(&def_id)?.clone();

        Some(var.get_ref())
    }

    fn check_array(&mut self, root: LinkedNode<'_>) -> Option<FlowType> {
        let _arr: ast::Array = root.cast()?;

        Some(FlowType::Array)
    }

    fn check_dict(&mut self, root: LinkedNode<'_>) -> Option<FlowType> {
        let _dict: ast::Dict = root.cast()?;

        Some(FlowType::Dict)
    }

    fn check_unary(&mut self, root: LinkedNode<'_>) -> Option<FlowType> {
        let unary: ast::Unary = root.cast()?;

        if let Some(constant) = self.ctx.mini_eval(ast::Expr::Unary(unary)) {
            return Some(FlowType::Value(Box::new(constant)));
        }

        let op = unary.op();

        let expr = Box::new(self.check_expr_in(unary.expr().span(), root));
        let ty = match op {
            ast::UnOp::Pos => FlowUnaryType::Pos(expr),
            ast::UnOp::Neg => FlowUnaryType::Neg(expr),
            ast::UnOp::Not => FlowUnaryType::Not(expr),
        };

        Some(FlowType::Unary(ty))
    }

    fn check_binary(&mut self, root: LinkedNode<'_>) -> Option<FlowType> {
        let binary: ast::Binary = root.cast()?;

        if let Some(constant) = self.ctx.mini_eval(ast::Expr::Binary(binary)) {
            return Some(FlowType::Value(Box::new(constant)));
        }

        let op = binary.op();
        let lhs = self.check_expr_in(binary.lhs().span(), root.clone());
        let rhs = self.check_expr_in(binary.rhs().span(), root);
        let repr = FlowBinaryRepr(Box::new((lhs, rhs)));

        let ty = match op {
            ast::BinOp::Add => FlowBinaryType::Add(repr),
            ast::BinOp::Sub => FlowBinaryType::Sub(repr),
            ast::BinOp::Mul => FlowBinaryType::Mul(repr),
            ast::BinOp::Div => FlowBinaryType::Div(repr),
            ast::BinOp::And => FlowBinaryType::And(repr),
            ast::BinOp::Or => FlowBinaryType::Or(repr),
            ast::BinOp::Eq => FlowBinaryType::Eq(repr),
            ast::BinOp::Neq => FlowBinaryType::Neq(repr),
            ast::BinOp::Lt => FlowBinaryType::Lt(repr),
            ast::BinOp::Leq => FlowBinaryType::Leq(repr),
            ast::BinOp::Gt => FlowBinaryType::Gt(repr),
            ast::BinOp::Geq => FlowBinaryType::Geq(repr),
            ast::BinOp::Assign => FlowBinaryType::Assign(repr),
            ast::BinOp::In => FlowBinaryType::In(repr),
            ast::BinOp::NotIn => FlowBinaryType::NotIn(repr),
            ast::BinOp::AddAssign => FlowBinaryType::AddAssign(repr),
            ast::BinOp::SubAssign => FlowBinaryType::SubAssign(repr),
            ast::BinOp::MulAssign => FlowBinaryType::MulAssign(repr),
            ast::BinOp::DivAssign => FlowBinaryType::DivAssign(repr),
        };

        // Some(FlowType::Binary(ty))
        Some(FlowType::Binary(ty))
    }

    fn check_field_access(&mut self, root: LinkedNode<'_>) -> Option<FlowType> {
        let field_access: ast::FieldAccess = root.cast()?;

        let obj = self.check_expr_in(field_access.target().span(), root.clone());
        let field = field_access.field().get().clone();

        Some(FlowType::At(FlowAt(Box::new((obj, field)))))
    }

    fn check_func_call(&mut self, root: LinkedNode<'_>) -> Option<FlowType> {
        let func_call: ast::FuncCall = root.cast()?;

        let args = self.check_expr_in(func_call.args().span(), root.clone());
        let callee = self.check_expr_in(func_call.callee().span(), root.clone());
        let mut candidates = Vec::with_capacity(1);

        log::debug!("func_call: {callee:?} with {args:?}");

        if let FlowType::Args(args) = args {
            self.check_apply(callee, &args, &func_call.args(), &mut candidates)?;
        }

        if candidates.len() == 1 {
            return Some(candidates[0].clone());
        }

        if candidates.is_empty() {
            return Some(FlowType::Any);
        }

        Some(FlowType::Union(Box::new(candidates)))
    }

    fn check_args(&mut self, root: LinkedNode<'_>) -> Option<FlowType> {
        let args: ast::Args = root.cast()?;

        let mut args_res = Vec::new();
        let mut named = vec![];

        for arg in args.items() {
            match arg {
                ast::Arg::Pos(e) => {
                    args_res.push(self.check_expr_in(e.span(), root.clone()));
                }
                ast::Arg::Named(n) => {
                    let name = n.name().get().clone();
                    let value = self.check_expr_in(n.expr().span(), root.clone());
                    named.push((name, value));
                }
                // todo
                ast::Arg::Spread(_w) => {}
            }
        }

        Some(FlowType::Args(Box::new(FlowArgs {
            args: args_res,
            named,
        })))
    }

    fn check_closure(&mut self, root: LinkedNode<'_>) -> Option<FlowType> {
        let closure: ast::Closure = root.cast()?;

        // let _params = self.check_expr_in(closure.params().span(), root.clone());

        let mut pos = vec![];
        let mut named = HashMap::new();
        let mut rest = None;

        for param in closure.params().children() {
            match param {
                ast::Param::Pos(pattern) => {
                    pos.push(self.check_pattern(pattern, FlowType::Any, root.clone()));
                }
                ast::Param::Named(e) => {
                    let exp = self.check_expr_in(e.span(), root.clone());
                    let v = self.get_var(e.span(), to_ident_ref(&root, e.name())?)?;
                    v.ever_be(exp);
                    named.insert(e.name().get().clone(), v.get_ref());
                }
                ast::Param::Spread(a) => {
                    if let Some(e) = a.sink_ident() {
                        let exp = FlowType::Builtin(FlowBuiltinType::Args);
                        let v = self.get_var(e.span(), to_ident_ref(&root, e)?)?;
                        v.ever_be(exp);
                        rest = Some(v.get_ref());
                    }
                    // todo: ..(args)
                }
            }
        }

        let body = self.check_expr_in(closure.body().span(), root);

        Some(FlowType::Func(Box::new(FlowSignature {
            pos,
            named: named.into_iter().collect(),
            rest,
            ret: body,
        })))
    }

    fn check_let(&mut self, root: LinkedNode<'_>) -> Option<FlowType> {
        let let_binding: ast::LetBinding = root.cast()?;

        match let_binding.kind() {
            ast::LetBindingKind::Closure(c) => {
                // let _name = let_binding.name().get().to_string();
                let value = let_binding
                    .init()
                    .map(|init| self.check_expr_in(init.span(), root.clone()))
                    .unwrap_or_else(|| FlowType::Infer);

                let v = self.get_var(c.span(), to_ident_ref(&root, c)?)?;
                v.as_strong(value);
                // todo lbs is the lexical signature.
            }
            ast::LetBindingKind::Normal(pattern) => {
                // let _name = let_binding.name().get().to_string();
                let value = let_binding
                    .init()
                    .map(|init| self.check_expr_in(init.span(), root.clone()))
                    .unwrap_or_else(|| FlowType::Infer);

                self.check_pattern(pattern, value, root.clone());
            }
        }

        Some(FlowType::Any)
    }

    fn check_set(&mut self, root: LinkedNode<'_>) -> Option<FlowType> {
        let set_rule: ast::SetRule = root.cast()?;

        let _target = self.check_expr_in(set_rule.target().span(), root.clone());
        let _args = self.check_expr_in(set_rule.args().span(), root);

        Some(FlowType::Any)
    }

    fn check_show(&mut self, root: LinkedNode<'_>) -> Option<FlowType> {
        let show_rule: ast::ShowRule = root.cast()?;

        let _selector = show_rule
            .selector()
            .map(|sel| self.check_expr_in(sel.span(), root.clone()));
        // let _args = self.check_expr_in(show_rule.args().span(), root)?;

        Some(FlowType::Any)
    }

    fn check_contextual(&mut self, root: LinkedNode<'_>) -> Option<FlowType> {
        let contextual: ast::Contextual = root.cast()?;

        let _body = self.check_expr_in(contextual.body().span(), root);

        Some(FlowType::Content)
    }

    fn check_conditional(&mut self, root: LinkedNode<'_>) -> Option<FlowType> {
        let conditional: ast::Conditional = root.cast()?;

        let _cond = self.check_expr_in(conditional.condition().span(), root.clone());
        let _then = self.check_expr_in(conditional.if_body().span(), root.clone());
        let _else = conditional
            .else_body()
            .map(|else_body| self.check_expr_in(else_body.span(), root.clone()))
            .unwrap_or(FlowType::None);

        Some(FlowType::Any)
    }

    fn check_while_loop(&mut self, root: LinkedNode<'_>) -> Option<FlowType> {
        let while_loop: ast::WhileLoop = root.cast()?;

        let _cond = self.check_expr_in(while_loop.condition().span(), root.clone());
        let _body = self.check_expr_in(while_loop.body().span(), root);

        Some(FlowType::Any)
    }

    fn check_for_loop(&mut self, root: LinkedNode<'_>) -> Option<FlowType> {
        let for_loop: ast::ForLoop = root.cast()?;

        let _iter = self.check_expr_in(for_loop.iterable().span(), root.clone());
        let _pattern = self.check_expr_in(for_loop.pattern().span(), root.clone());
        let _body = self.check_expr_in(for_loop.body().span(), root);

        Some(FlowType::Any)
    }

    fn check_module_import(&mut self, root: LinkedNode<'_>) -> Option<FlowType> {
        let _module_import: ast::ModuleImport = root.cast()?;

        // check all import items

        Some(FlowType::None)
    }

    fn check_module_include(&mut self, _root: LinkedNode<'_>) -> Option<FlowType> {
        Some(FlowType::Content)
    }

    fn check_destructuring(&mut self, _root: LinkedNode<'_>) -> Option<FlowType> {
        Some(FlowType::Any)
    }

    fn check_destruct_assign(&mut self, _root: LinkedNode<'_>) -> Option<FlowType> {
        Some(FlowType::None)
    }

    fn check_expr_in(&mut self, span: Span, root: LinkedNode<'_>) -> FlowType {
        root.find(span)
            .map(|node| self.check(node))
            .unwrap_or(FlowType::Undef)
    }

    fn get_var(&mut self, s: Span, r: IdentRef) -> Option<&mut FlowVar> {
        let def_id = self
            .def_use_info
            .get_ref(&r)
            .or_else(|| Some(self.def_use_info.get_def(s.id()?, &r)?.0))?;

        let var = self.info.vars.entry(def_id).or_insert_with(|| {
            // let store = FlowVarStore {
            //     name: r.name.into(),
            //     id: def_id,
            //     lbs: Vec::new(),
            //     ubs: Vec::new(),
            // };
            // FlowVar(Arc::new(RwLock::new(store)))
            FlowVar {
                name: r.name.into(),
                id: def_id,
                kind: FlowVarKind::Weak(Arc::new(RwLock::new(FlowVarStore {
                    lbs: Vec::new(),
                    ubs: Vec::new(),
                }))),
                // kind: FlowVarKind::Strong(FlowType::Any),
            }
        });

        self.info.mapping.insert(s, var.get_ref());
        Some(var)
    }

    fn check_pattern(
        &mut self,
        pattern: ast::Pattern<'_>,
        value: FlowType,
        root: LinkedNode<'_>,
    ) -> FlowType {
        self.check_pattern_(pattern, value, root)
            .unwrap_or(FlowType::Undef)
    }

    fn check_pattern_(
        &mut self,
        pattern: ast::Pattern<'_>,
        value: FlowType,
        root: LinkedNode<'_>,
    ) -> Option<FlowType> {
        Some(match pattern {
            ast::Pattern::Normal(ast::Expr::Ident(ident)) => {
                let v = self.get_var(ident.span(), to_ident_ref(&root, ident)?)?;
                v.ever_be(value);
                v.get_ref()
            }
            ast::Pattern::Normal(_) => FlowType::Any,
            ast::Pattern::Placeholder(_) => FlowType::Any,
            ast::Pattern::Parenthesized(exp) => self.check_pattern(exp.pattern(), value, root),
            // todo: pattern
            ast::Pattern::Destructuring(_destruct) => FlowType::Any,
        })
    }

    fn check_apply(
        &mut self,
        callee: FlowType,
        args: &FlowArgs,
        syntax_args: &ast::Args,
        candidates: &mut Vec<FlowType>,
    ) -> Option<()> {
        // println!("check func callee {callee:?}");

        match &callee {
            FlowType::Var(v) => {
                let w = self.info.vars.get(&v.0).cloned()?;
                match &w.kind {
                    FlowVarKind::Weak(w) => {
                        let w = w.read();
                        for lb in w.lbs.iter() {
                            self.check_apply(lb.clone(), args, syntax_args, candidates)?;
                        }
                        for ub in w.ubs.iter() {
                            self.check_apply(ub.clone(), args, syntax_args, candidates)?;
                        }
                    }
                }
            }
            FlowType::Func(v) => {
                let f = v.as_ref();
                let mut pos = f.pos.iter();
                // let mut named = f.named.clone();
                // let mut rest = f.rest.clone();

                for pos_in in args.start_match() {
                    let pos_ty = pos.next().unwrap_or(&FlowType::Any);
                    self.constrain(pos_in, pos_ty);
                }

                for (name, named_in) in &args.named {
                    let named_ty = f.named.iter().find(|(n, _)| n == name).map(|(_, ty)| ty);
                    if let Some(named_ty) = named_ty {
                        self.constrain(named_in, named_ty);
                    }
                }

                // println!("check applied {v:?}");

                candidates.push(f.ret.clone());
            }
            // todo: with
            FlowType::With(_e) => {}
            FlowType::Args(_e) => {}
            FlowType::Union(_e) => {}
            FlowType::Let(_) => {}
            FlowType::Value(f) => {
                if let Value::Func(f) = f.as_ref() {
                    self.check_apply_runtime(f, args, syntax_args, candidates);
                }
            }
            FlowType::ValueDoc(f) => {
                if let Value::Func(f) = &f.0 {
                    self.check_apply_runtime(f, args, syntax_args, candidates);
                }
            }

            FlowType::Array => {}
            FlowType::Dict => {}
            FlowType::Clause => {}
            FlowType::Undef => {}
            FlowType::Content => {}
            FlowType::Any => {}
            FlowType::None => {}
            FlowType::Infer => {}
            FlowType::FlowNone => {}
            FlowType::Auto => {}
            FlowType::Builtin(_) => {}
            FlowType::At(e) => {
                let primary_type = self.check_primary_type(e.0 .0.clone());
                self.check_apply_method(primary_type, e.0 .1.clone(), args, candidates);
            }
            FlowType::Unary(_) => {}
            FlowType::Binary(_) => {}
            FlowType::Element(_elem) => {}
        }

        Some(())
    }

    fn constrain(&mut self, lhs: &FlowType, rhs: &FlowType) {
        match (lhs, rhs) {
            (FlowType::Var(v), FlowType::Var(w)) => {
                if v.0 .0 == w.0 .0 {
                    return;
                }

                // todo: merge

                let _ = v.0 .0;
                let _ = w.0 .0;
            }
            (FlowType::Var(v), rhs) => {
                let w = self.info.vars.get_mut(&v.0).unwrap();
                match &w.kind {
                    FlowVarKind::Weak(w) => {
                        let mut w = w.write();
                        w.ubs.push(rhs.clone());
                    }
                }
            }
            (_, FlowType::Var(v)) => {
                let v = self.info.vars.get(&v.0).unwrap();
                match &v.kind {
                    FlowVarKind::Weak(v) => {
                        let mut v = v.write();
                        v.lbs.push(lhs.clone());
                    }
                }
            }
            _ => {}
        }
    }

    fn check_primary_type(&self, e: FlowType) -> FlowType {
        match &e {
            FlowType::Var(v) => {
                let w = self.info.vars.get(&v.0).unwrap();
                match &w.kind {
                    FlowVarKind::Weak(w) => {
                        let w = w.read();
                        if !w.ubs.is_empty() {
                            return w.ubs[0].clone();
                        }
                        if !w.lbs.is_empty() {
                            return w.lbs[0].clone();
                        }
                        FlowType::Any
                    }
                }
            }
            FlowType::Func(..) => e,
            FlowType::With(..) => e,
            FlowType::Args(..) => e,
            FlowType::Union(..) => e,
            FlowType::Let(_) => e,
            FlowType::Value(..) => e,
            FlowType::ValueDoc(..) => e,

            FlowType::Array => e,
            FlowType::Dict => e,
            FlowType::Clause => e,
            FlowType::Undef => e,
            FlowType::Content => e,
            FlowType::Any => e,
            FlowType::None => e,
            FlowType::Infer => e,
            FlowType::FlowNone => e,
            FlowType::Auto => e,
            FlowType::Builtin(_) => e,
            FlowType::At(e) => self.check_primary_type(e.0 .0.clone()),
            FlowType::Unary(_) => e,
            FlowType::Binary(_) => e,
            FlowType::Element(_) => e,
        }
    }

    fn check_apply_method(
        &mut self,
        primary_type: FlowType,
        method_name: EcoString,
        args: &FlowArgs,
        _candidates: &mut Vec<FlowType>,
    ) -> Option<()> {
        log::debug!("check method at {method_name:?} on {primary_type:?}");
        match primary_type {
            FlowType::Func(v) => match method_name.as_str() {
                "with" => {
                    // println!("check method at args: {v:?}.with({args:?})");

                    let f = v.as_ref();
                    let mut pos = f.pos.iter();
                    // let mut named = f.named.clone();
                    // let mut rest = f.rest.clone();

                    for pos_in in args.start_match() {
                        let pos_ty = pos.next().unwrap_or(&FlowType::Any);
                        self.constrain(pos_in, pos_ty);
                    }

                    for (name, named_in) in &args.named {
                        let named_ty = f.named.iter().find(|(n, _)| n == name).map(|(_, ty)| ty);
                        if let Some(named_ty) = named_ty {
                            self.constrain(named_in, named_ty);
                        }
                    }

                    _candidates.push(self.partial_apply(f, args));
                }
                "where" => {
                    // println!("where method at args: {args:?}");
                }
                _ => {}
            },
            FlowType::Array => {}
            _ => {}
        }

        Some(())
    }

    fn check_apply_runtime(
        &mut self,
        f: &Func,
        args: &FlowArgs,
        syntax_args: &ast::Args,
        candidates: &mut Vec<FlowType>,
    ) -> Option<()> {
        let sig = analyze_dyn_signature(self.ctx, f.clone());

        log::debug!("check runtime func {f:?} at args: {args:?}");

        let mut pos = sig
            .primary()
            .pos
            .iter()
            .map(|e| e.infer_type.as_ref().unwrap_or(&FlowType::Any));
        let mut syntax_pos = syntax_args.items().filter_map(|arg| match arg {
            ast::Arg::Pos(e) => Some(e),
            _ => None,
        });

        for pos_in in args.start_match() {
            let pos_ty = pos.next().unwrap_or(&FlowType::Any);
            self.constrain(pos_in, pos_ty);
            if let Some(syntax_pos) = syntax_pos.next() {
                self.info.mapping.insert(syntax_pos.span(), pos_ty.clone());
            }
        }

        for (name, named_in) in &args.named {
            let named_ty = sig
                .primary()
                .named
                .get(name.as_ref())
                .and_then(|e| e.infer_type.as_ref());
            let syntax_named = syntax_args
                .items()
                .filter_map(|arg| match arg {
                    ast::Arg::Named(n) => Some(n),
                    _ => None,
                })
                .find(|n| n.name().get() == name.as_ref());
            if let Some(named_ty) = named_ty {
                self.constrain(named_in, named_ty);
            }
            if let Some(syntax_named) = syntax_named {
                self.info
                    .mapping
                    .insert(syntax_named.span(), named_in.clone());
            }
        }

        candidates.push(sig.primary().ret_ty.clone().unwrap_or(FlowType::Any));

        Some(())
    }

    fn partial_apply(&self, f: &FlowSignature, args: &FlowArgs) -> FlowType {
        FlowType::With(Box::new((
            FlowType::Func(Box::new(f.clone())),
            vec![args.clone()],
        )))
    }
}

#[derive(Default)]
struct TypeCanoStore {
    cano_cache: HashMap<u128, FlowType>,
    cano_local_cache: HashMap<DefId, FlowType>,
    negatives: HashSet<DefId>,
    positives: HashSet<DefId>,
}

struct TypeSimplifier<'a, 'b> {
    vars: &'a HashMap<DefId, FlowVar>,

    cano_cache: &'b mut HashMap<u128, FlowType>,
    cano_local_cache: &'b mut HashMap<DefId, FlowType>,
    negatives: &'b mut HashSet<DefId>,
    positives: &'b mut HashSet<DefId>,
}

impl<'a, 'b> TypeSimplifier<'a, 'b> {
    fn simplify(&mut self, ty: FlowType) -> FlowType {
        // todo: hash safety
        let ty_key = hash128(&ty);
        if let Some(cano) = self.cano_cache.get(&ty_key) {
            return cano.clone();
        }

        self.analyze(&ty, true);

        self.transform(&ty, true)
    }

    fn analyze(&mut self, ty: &FlowType, pol: bool) {
        match ty {
            FlowType::Var(v) => {
                let w = self.vars.get(&v.0).unwrap();
                match &w.kind {
                    FlowVarKind::Weak(w) => {
                        let w = w.read();
                        if pol {
                            self.positives.insert(v.0);
                        } else {
                            self.negatives.insert(v.0);
                        }

                        if pol {
                            for lb in w.lbs.iter() {
                                self.analyze(lb, pol);
                            }
                        } else {
                            for ub in w.ubs.iter() {
                                self.analyze(ub, pol);
                            }
                        }
                    }
                }
            }
            FlowType::Func(f) => {
                for p in &f.pos {
                    self.analyze(p, !pol);
                }
                for (_, p) in &f.named {
                    self.analyze(p, !pol);
                }
                if let Some(r) = &f.rest {
                    self.analyze(r, !pol);
                }
                self.analyze(&f.ret, pol);
            }
            FlowType::With(w) => {
                self.analyze(&w.0, pol);
                for m in &w.1 {
                    for arg in m.args.iter() {
                        self.analyze(arg, pol);
                    }
                }
            }
            FlowType::Args(args) => {
                for arg in &args.args {
                    self.analyze(arg, pol);
                }
            }
            FlowType::Unary(u) => self.analyze(u.lhs(), pol),
            FlowType::Binary(b) => {
                let repr = b.repr();
                self.analyze(&repr.0 .0, pol);
                self.analyze(&repr.0 .1, pol);
            }
            FlowType::Union(v) => {
                for ty in v.iter() {
                    self.analyze(ty, pol);
                }
            }
            FlowType::At(a) => {
                self.analyze(&a.0 .0, pol);
            }
            FlowType::Let(v) => {
                for lb in v.lbs.iter() {
                    self.analyze(lb, !pol);
                }
                for ub in v.ubs.iter() {
                    self.analyze(ub, pol);
                }
            }
            FlowType::Value(_v) => {}
            FlowType::ValueDoc(_v) => {}
            FlowType::Clause => {}
            FlowType::Undef => {}
            FlowType::Content => {}
            FlowType::Any => {}
            FlowType::None => {}
            FlowType::Infer => {}
            FlowType::FlowNone => {}
            FlowType::Auto => {}
            FlowType::Builtin(_) => {}
            // todo
            FlowType::Array => {}
            FlowType::Dict => {}
            FlowType::Element(_) => {}
        }
    }

    fn transform(&mut self, ty: &FlowType, pol: bool) -> FlowType {
        match ty {
            FlowType::Var(v) => {
                if let Some(cano) = self.cano_local_cache.get(&v.0) {
                    return cano.clone();
                }

                match &self.vars.get(&v.0).unwrap().kind {
                    FlowVarKind::Weak(w) => {
                        let w = w.read();

                        // println!("transform var {:?} {pol}", v.0);

                        let mut lbs = Vec::with_capacity(w.lbs.len());
                        let mut ubs = Vec::with_capacity(w.ubs.len());

                        if pol && !self.negatives.contains(&v.0) {
                            for lb in w.lbs.iter() {
                                lbs.push(self.transform(lb, pol));
                            }
                        }
                        if !pol && !self.positives.contains(&v.0) {
                            for ub in w.ubs.iter() {
                                ubs.push(self.transform(ub, !pol));
                            }
                        }

                        if ubs.is_empty() {
                            if lbs.len() == 1 {
                                return lbs.pop().unwrap();
                            }
                            if lbs.is_empty() {
                                return FlowType::Any;
                            }
                        }

                        FlowType::Let(Arc::new(FlowVarStore {
                            lbs: w.lbs.clone(),
                            ubs: w.ubs.clone(),
                        }))
                    }
                }
            }
            FlowType::Func(f) => {
                let pos = f.pos.iter().map(|p| self.transform(p, !pol)).collect();
                let named = f
                    .named
                    .iter()
                    .map(|(n, p)| (n.clone(), self.transform(p, !pol)))
                    .collect();
                let rest = f.rest.as_ref().map(|r| self.transform(r, !pol));
                let ret = self.transform(&f.ret, pol);

                FlowType::Func(Box::new(FlowSignature {
                    pos,
                    named,
                    rest,
                    ret,
                }))
            }
            FlowType::With(w) => {
                let primary = self.transform(&w.0, pol);
                FlowType::With(Box::new((primary, w.1.clone())))
            }
            FlowType::Args(args) => {
                let args_res = args.args.iter().map(|a| self.transform(a, pol)).collect();
                let named = args
                    .named
                    .iter()
                    .map(|(n, a)| (n.clone(), self.transform(a, pol)))
                    .collect();

                FlowType::Args(Box::new(FlowArgs {
                    args: args_res,
                    named,
                }))
            }
            FlowType::Unary(u) => {
                let u2 = u.clone();

                FlowType::Unary(u2)
            }
            FlowType::Binary(b) => {
                let b2 = b.clone();

                FlowType::Binary(b2)
            }
            FlowType::Union(v) => {
                let v2 = v.iter().map(|ty| self.transform(ty, pol)).collect();

                FlowType::Union(Box::new(v2))
            }
            FlowType::At(a) => {
                let a2 = a.clone();

                FlowType::At(a2)
            }
            // todo
            FlowType::Let(_) => FlowType::Any,
            FlowType::Array => FlowType::Array,
            FlowType::Dict => FlowType::Dict,
            FlowType::Value(v) => FlowType::Value(v.clone()),
            FlowType::ValueDoc(v) => FlowType::ValueDoc(v.clone()),
            FlowType::Element(v) => FlowType::Element(*v),
            FlowType::Clause => FlowType::Clause,
            FlowType::Undef => FlowType::Undef,
            FlowType::Content => FlowType::Content,
            FlowType::Any => FlowType::Any,
            FlowType::None => FlowType::None,
            FlowType::Infer => FlowType::Infer,
            FlowType::FlowNone => FlowType::FlowNone,
            FlowType::Auto => FlowType::Auto,
            FlowType::Builtin(b) => FlowType::Builtin(b.clone()),
        }
    }
}

fn to_ident_ref(root: &LinkedNode, c: ast::Ident) -> Option<IdentRef> {
    Some(IdentRef {
        name: c.get().to_string(),
        range: root.find(c.span())?.range(),
    })
}
