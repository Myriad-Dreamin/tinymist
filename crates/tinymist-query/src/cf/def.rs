use tinymist_std::adt::IndexVecIdx;
use typst::foundations::Element;
use typst::syntax::Span;

use super::CfRepr;
use crate::adt::{FromIndex, IndexVec};
use crate::syntax::{BinaryOp, Decl, DeclExpr, Expr, Pattern, UnaryOp};
use crate::ty::Interned;
use crate::ty::Ty;

pub struct CfInfo {
    pub region: IndexVec<Region>,
    pub root: RegionId,
}

impl CfInfo {
    /// Creates a new CfInfo.
    pub fn new() -> Self {
        let mut region = IndexVec::<Region>::new();
        let root = region.create().id;

        Self { region, root }
    }

    pub fn repr(&self, spaned: bool) -> CfRepr<'_> {
        CfRepr { cf: self, spaned }
    }
}

impl Default for CfInfo {
    fn default() -> Self {
        Self::new()
    }
}

pub type NodeId = IndexVecIdx<CfNode>;
pub type BasicBlockId = IndexVecIdx<BasicBlock>;
pub type RegionId = IndexVecIdx<Region>;

pub struct Region {
    pub id: RegionId,
    pub basic_blocks: IndexVec<BasicBlock>,
    pub nodes: IndexVec<CfNode>,
}

impl FromIndex for Region {
    fn from_index(id: IndexVecIdx<Self>) -> Self {
        Self {
            id,
            basic_blocks: IndexVec::new(),
            nodes: IndexVec::new(),
        }
    }
}

pub struct BasicBlock {
    pub id: BasicBlockId,
    pub nodes: Vec<NodeId>,
}

impl FromIndex for BasicBlock {
    fn from_index(id: IndexVecIdx<Self>) -> Self {
        Self {
            id,
            nodes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CfNode {
    pub span: Span,
    pub instr: CfInstr,
}

impl CfNode {
    pub fn detached(instr: CfInstr) -> Self {
        Self {
            span: Span::detached(),
            instr,
        }
    }
}

#[derive(Debug, Clone)]
pub enum CfInstr {
    Let(CfLet),
    Assign(CfAssign),
    Bin(CfBin),
    Un(CfUn),
    Select(CfSelect),
    Apply(CfCall),
    Func(CfFunc),
    Array(CfArgs),
    Dict(CfArgs),
    Args(CfArgs),
    If(CfIf),
    Loop(CfLoop),
    Block(CfBlock),
    Element(CfElement),
    Contextual(BasicBlockId),
    Show(CfShow),
    Set(CfSet),
    Break(Option<BasicBlockId>),
    Continue(Option<BasicBlockId>),
    Branch(BasicBlockId),
    Meta(Expr),
    Undef(DeclExpr),
    Ins(Ty),
    Include(NodeId),
    Iter(NodeId),
    CovPoint(Span),
    Return(Option<NodeId>),
}

#[derive(Debug, Clone)]
pub struct CfLet {
    pub ty: Ty,
    pub pattern: Interned<Pattern>,
    pub init: Option<NodeId>,
}

#[derive(Debug, Clone)]
pub struct CfAssign {
    pub lhs: NodeId,
    pub rhs: NodeId,
}

#[derive(Debug, Clone)]
pub struct CfBin {
    pub ty: Ty,
    pub op: BinaryOp,
    pub lhs: NodeId,
    pub rhs: NodeId,
}

pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    And,
    Or,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone)]
pub struct CfUn {
    pub ty: Ty,
    pub op: UnaryOp,
    pub lhs: NodeId,
}

#[derive(Debug, Clone)]
pub struct CfSelect {
    pub ty: Ty,
    pub lhs: NodeId,
    pub key: Interned<Decl>,
}

pub enum UnOp {
    Neg,
    Not,
}

#[derive(Debug, Clone)]
pub struct CfCall {
    pub ty: Ty,
    pub func: NodeId,
    pub args: NodeId,
}

#[derive(Debug, Clone)]
pub struct CfFunc {
    pub ty: Ty,
    pub body: RegionId,
}
#[derive(Debug, Clone)]

pub struct CfArgs {
    pub ty: Ty,
    pub args: Vec<CfArg>,
}

#[derive(Debug, Clone)]
pub enum CfArg {
    Pos(NodeId),
    Named(Interned<Decl>, NodeId),
    NamedRt(NodeId, NodeId),
    Spread(NodeId),
}

#[derive(Debug, Clone)]
pub struct CfIf {
    pub ty: Ty,
    pub cond: NodeId,
    pub then: Box<CfNode>,
    pub else_: Box<CfNode>,
}
#[derive(Debug, Clone)]

pub struct CfLoop {
    pub ty: Ty,
    pub cond: NodeId,
    pub body: BasicBlockId,
    pub cont: Option<BasicBlockId>,
}

#[derive(Debug, Clone)]
pub struct CfBlock {
    pub ty: Ty,
    pub body: BasicBlockId,
}

#[derive(Debug, Clone)]
pub struct CfElement {
    pub elem: Element,
    pub body: BasicBlockId,
}

#[derive(Debug, Clone)]
pub struct CfShow {
    pub selector: Option<NodeId>,
    pub edit: NodeId,
    pub cont: BasicBlockId,
}

#[derive(Debug, Clone)]
pub struct CfSet {
    pub target: NodeId,
    pub args: NodeId,
    pub cont: BasicBlockId,
}
