use typst::syntax::Span;

use crate::ty::Ty;

pub struct ControlFlowInfo {
    pub region: Vec<Region>,
}

type NodeId = usize;
type BlockId = usize;
type RegionId = usize;

pub struct Region {
    pub id: RegionId,
    pub basic_blocks: Vec<BasicBlock>,
}

pub struct BasicBlock {
    pub id: BlockId,
    pub nodes: Vec<CfNode>,
}

pub struct CfNode {
    pub id: NodeId,
    pub span: Span,
    pub instr: CfInstr,
}

pub enum CfInstr {
    Let(CfLet),
    Assign(CfAssign),
    Bin(CfBin),
    Un(CfUn),
    Call(CfCall),
    Func(CfFunc),
    If(CfIf),
    Loop(CfLoop),
    Show(CfShow),
    Set(CfSet),
    Break(CfBreak),
    Continue(CfContinue),
    Return(CfReturn),
}

pub struct CfLet {
    pub ty: Ty,
    pub init: Option<NodeId>,
}

pub struct CfAssign {
    pub lhs: NodeId,
    pub rhs: NodeId,
}

pub struct CfBin {
    pub ty: Ty,
    pub op: BinOp,
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

pub struct CfUn {
    pub ty: Ty,
    pub op: UnOp,
    pub arg: NodeId,
}

pub enum UnOp {
    Neg,
    Not,
}

pub struct CfCall {
    pub ty: Ty,
    pub func: NodeId,
    pub args: Vec<NodeId>,
}

pub struct CfFunc {
    pub ty: Ty,
    pub body: RegionId,
}

pub struct CfIf {
    pub ty: Ty,
    pub cond: NodeId,
    pub then: BlockId,
    pub else_: Option<BlockId>,
}

pub struct CfLoop {
    pub ty: Ty,
    pub body: BlockId,
    pub cont: Option<BlockId>,
}

pub struct CfShow {
    pub rule: NodeId,
    pub cont: BlockId,
}

pub struct CfSet {
    pub rule: NodeId,
    pub cont: BlockId,
}

pub struct CfBreak {
    pub cont: NodeId,
}

pub struct CfContinue {
    pub cont: NodeId,
}

pub struct CfReturn {
    pub arg: Option<NodeId>,
}
