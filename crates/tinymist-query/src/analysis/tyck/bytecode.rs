//! Experimental bytecode model for type-level evaluation.
//!
//! This module is intentionally not wired into the checker yet. It defines the
//! compact program model used by later type VM work while preserving `Ty` as the
//! external representation.

#![allow(dead_code)]

use ecow::EcoString;
use tinymist_analysis::adt::interner::Interned;
use typst::syntax::ast::BinOp;

use crate::{
    syntax::def::{
        ApplyExpr, ArgExpr, ArgsExpr, BinExpr, DeclExpr, Expr, FuncExpr, IfExpr, Pattern,
        PatternSig, SelectExpr, UnExpr, UnaryOp,
    },
    ty::{ArgsTy, IfTy, SelectTy, SigTy, Ty, TypeBinary, TypeUnary},
};

type StrRef = Interned<str>;

/// Identifier for a bytecode constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ConstId(pub(crate) u32);

/// Identifier for a local slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct LocalId(pub(crate) u32);

/// Identifier for a closure prototype in a program.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct ClosureProtoId(pub(crate) u32);

/// Identifier for a type meta variable in the semantic value domain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct MetaId(pub(crate) u32);

/// A bytecode program for type-level evaluation.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct TyProgram {
    /// Constant pool.
    pub(crate) consts: Vec<TyConst>,
    /// Closure prototypes owned by this program.
    pub(crate) closures: Vec<ClosureProto>,
    /// Program instructions.
    pub(crate) code: Vec<TyInstr>,
}

impl TyProgram {
    /// Adds a constant and returns its handle.
    pub(crate) fn push_const(&mut self, constant: TyConst) -> ConstId {
        let id = ConstId(self.consts.len() as u32);
        self.consts.push(constant);
        id
    }

    /// Adds a closure prototype and returns its handle.
    pub(crate) fn push_closure(&mut self, closure: ClosureProto) -> ClosureProtoId {
        let id = ClosureProtoId(self.closures.len() as u32);
        self.closures.push(closure);
        id
    }
}

/// Constants loaded by type bytecode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TyConst {
    /// A concrete `Ty` value.
    Type(Ty),
    /// An interned string.
    Str(StrRef),
    /// The `none` value at the type level.
    None,
    /// The top type.
    Any,
}

/// Type bytecode instructions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TyInstr {
    /// Push a constant onto the stack.
    LoadConst(ConstId),
    /// Push a local onto the stack.
    LoadLocal(LocalId),
    /// Push a global declaration reference onto the stack.
    LoadGlobal(DeclExpr),
    /// Store the top of the stack into a local.
    StoreLocal(LocalId),
    /// Push a captured value.
    LoadCapture(u32),
    /// Pop the top value.
    Pop,
    /// Build positional and named arguments from stack values.
    MakeArgs(ArgsShape),
    /// Build an array from stack values.
    MakeArray { len: u32 },
    /// Build a dictionary from stack key/value pairs or known named fields.
    MakeDict { len: u32 },
    /// Build a tuple from stack values.
    MakeTuple { len: u32 },
    /// Create a closure from a prototype.
    MakeClosure(ClosureProtoId),
    /// Call a callee with an argument value.
    Call,
    /// Select a field from a value.
    Select(StrRef),
    /// Apply a unary operation.
    Unary(UnaryOp),
    /// Apply a binary operation.
    Binary(BinOp),
    /// Jump if the top stack value is false-like.
    JumpIfFalse { target: u32 },
    /// Unconditional jump.
    Jump { target: u32 },
    /// Join `count` branch result values.
    Join { count: u32 },
    /// Return the top stack value.
    Return,
}

/// Shape of an argument object constructed from stack values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArgsShape {
    /// Number of positional values on the stack.
    pub(crate) positional: u32,
    /// Named argument keys in stack order.
    pub(crate) named: Vec<StrRef>,
    /// Number of spread argument values on the stack.
    pub(crate) spreads: u32,
}

/// A function closure prototype.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClosureProto {
    /// Declaration identifying the closure.
    pub(crate) decl: DeclExpr,
    /// Parameter metadata copied from syntax.
    pub(crate) params: ClosureParams,
    /// Captured declarations needed by the closure body.
    pub(crate) captures: Vec<DeclExpr>,
    /// Return meta assigned to this closure.
    pub(crate) ret: MetaId,
    /// Body bytecode.
    pub(crate) body: TyProgram,
}

/// Parameter metadata for a closure prototype.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClosureParams {
    /// Positional parameter declarations or pattern placeholders.
    pub(crate) positional: Vec<Option<DeclExpr>>,
    /// Named parameter declarations.
    pub(crate) named: Vec<DeclExpr>,
    /// Left spread parameter.
    pub(crate) spread_left: Option<DeclExpr>,
    /// Right spread parameter.
    pub(crate) spread_right: Option<DeclExpr>,
}

impl ClosureParams {
    fn from_sig(sig: &PatternSig) -> Self {
        Self {
            positional: sig.pos.iter().map(pattern_decl).collect(),
            named: sig.named.iter().map(|(decl, _)| decl.clone()).collect(),
            spread_left: sig.spread_left.as_ref().map(|(decl, _)| decl.clone()),
            spread_right: sig.spread_right.as_ref().map(|(decl, _)| decl.clone()),
        }
    }
}

fn pattern_decl(pattern: &Interned<Pattern>) -> Option<DeclExpr> {
    match pattern.as_ref() {
        Pattern::Simple(decl) => Some(decl.clone()),
        _ => None,
    }
}

/// Semantic type VM value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SemValue {
    /// Top type.
    Any,
    /// The `none` value.
    None,
    /// A computed type.
    Type(Ty),
    /// A closure value.
    Closure(ClosureValue),
    /// An argument object.
    Args(SemArgs),
    /// A record value.
    Record(Vec<(StrRef, SemValue)>),
    /// An array value.
    Array(Vec<SemValue>),
    /// A tuple value.
    Tuple(Vec<SemValue>),
    /// A meta variable.
    Meta(MetaValue),
    /// A stuck expression.
    Neutral(NeutralValue),
}

/// Semantic closure value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClosureValue {
    /// Closure prototype.
    pub(crate) proto: ClosureProtoId,
    /// Captured values.
    pub(crate) captures: Vec<SemValue>,
}

/// Semantic argument value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SemArgs {
    /// Positional arguments.
    pub(crate) positional: Vec<SemValue>,
    /// Named arguments.
    pub(crate) named: Vec<(StrRef, SemValue)>,
    /// Spread arguments.
    pub(crate) spreads: Vec<SemValue>,
}

/// Semantic meta variable value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MetaValue {
    /// Meta identifier.
    pub(crate) id: MetaId,
    /// Resolved bound, if known.
    pub(crate) bound: Option<Ty>,
}

/// Stuck neutral operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NeutralValue {
    /// A global declaration whose type is not available yet.
    Global { decl: DeclExpr, known: Option<Ty> },
    /// A local slot whose value is not available yet.
    Local(LocalId),
    /// A meta variable used as an operation head.
    Meta(MetaId),
    /// A stuck call.
    Apply {
        callee: Box<SemValue>,
        args: SemArgs,
    },
    /// A stuck selection.
    Select { value: Box<SemValue>, field: StrRef },
    /// A stuck unary operation.
    Unary { op: UnaryOp, value: Box<SemValue> },
    /// A stuck binary operation.
    Binary {
        op: BinOp,
        lhs: Box<SemValue>,
        rhs: Box<SemValue>,
    },
    /// A stuck conditional.
    If {
        cond: Box<SemValue>,
        then: Box<SemValue>,
        else_: Box<SemValue>,
    },
}

/// Quotes semantic values back to `Ty`.
pub(crate) trait QuoteTy {
    /// Quote this value into the public type representation.
    fn quote_ty(&self) -> Ty;
}

impl QuoteTy for SemValue {
    fn quote_ty(&self) -> Ty {
        match self {
            SemValue::Any => Ty::Any,
            SemValue::None => Ty::Builtin(crate::ty::BuiltinTy::None),
            SemValue::Type(ty) => ty.clone(),
            SemValue::Closure(_closure) => {
                let params = std::iter::empty();
                let sig = SigTy::new(params, [], None, None, Some(Ty::Any));
                Ty::Func(Interned::new(sig))
            }
            SemValue::Args(args) => Ty::Args(args.quote_args()),
            SemValue::Record(fields) => Ty::Dict(crate::ty::RecordTy::new(Vec::from_iter(
                fields
                    .iter()
                    .map(|(name, value)| (name.clone(), value.quote_ty())),
            ))),
            SemValue::Array(values) => Ty::Array(Interned::new(Ty::from_types(
                values.iter().map(QuoteTy::quote_ty),
            ))),
            SemValue::Tuple(values) => Ty::Tuple(Interned::new(
                values.iter().map(QuoteTy::quote_ty).collect(),
            )),
            SemValue::Meta(meta) => meta.bound.clone().unwrap_or(Ty::Any),
            SemValue::Neutral(neutral) => neutral.quote_ty(),
        }
    }
}

impl QuoteTy for NeutralValue {
    fn quote_ty(&self) -> Ty {
        match self {
            NeutralValue::Global { known, .. } => known.clone().unwrap_or(Ty::Any),
            NeutralValue::Local(..) | NeutralValue::Meta(..) => Ty::Any,
            NeutralValue::Apply { callee, args } => match callee.quote_ty() {
                Ty::Func(sig) => Ty::With(crate::ty::SigWithTy::new(
                    Interned::new(Ty::Func(sig)),
                    args.quote_args(),
                )),
                ty => Ty::With(crate::ty::SigWithTy::new(
                    Interned::new(ty),
                    args.quote_args(),
                )),
            },
            NeutralValue::Select { value, field } => Ty::Select(SelectTy::new(
                Interned::new(value.quote_ty()),
                field.clone(),
            )),
            NeutralValue::Unary { op, value } => Ty::Unary(TypeUnary::new(*op, value.quote_ty())),
            NeutralValue::Binary { op, lhs, rhs } => {
                Ty::Binary(TypeBinary::new(*op, lhs.quote_ty(), rhs.quote_ty()))
            }
            NeutralValue::If { cond, then, else_ } => Ty::If(IfTy::new(
                Interned::new(cond.quote_ty()),
                Interned::new(then.quote_ty()),
                Interned::new(else_.quote_ty()),
            )),
        }
    }
}

impl SemArgs {
    fn quote_args(&self) -> Interned<ArgsTy> {
        let positional = self.positional.iter().map(QuoteTy::quote_ty);
        let named = self
            .named
            .iter()
            .map(|(name, value)| (name.clone(), value.quote_ty()));
        ArgsTy::new(positional, named, None, None, None).into()
    }
}

/// Compiler from syntax expressions to type bytecode.
#[derive(Default)]
pub(crate) struct TyBytecodeCompiler {
    next_meta: u32,
}

impl TyBytecodeCompiler {
    /// Compile a supported expression into a bytecode program.
    pub(crate) fn compile_expr(&mut self, expr: &Expr) -> TyProgram {
        let mut program = TyProgram::default();
        self.emit_expr(&mut program, expr);
        program.code.push(TyInstr::Return);
        program
    }

    fn emit_expr(&mut self, program: &mut TyProgram, expr: &Expr) {
        match expr {
            Expr::Block(exprs) => self.emit_block(program, exprs),
            Expr::Array(args) => self.emit_array(program, args),
            Expr::Dict(args) => self.emit_dict(program, args),
            Expr::Args(args) => self.emit_args(program, args),
            Expr::Unary(unary) => self.emit_unary(program, unary),
            Expr::Binary(binary) => self.emit_binary(program, binary),
            Expr::Apply(apply) => self.emit_apply(program, apply),
            Expr::Func(func) => self.emit_func(program, func),
            Expr::Let(let_expr) => {
                if let Some(body) = &let_expr.body {
                    self.emit_expr(program, body);
                } else {
                    self.emit_const(program, TyConst::None);
                }
            }
            Expr::Select(select) => self.emit_select(program, select),
            Expr::Conditional(if_expr) => self.emit_conditional(program, if_expr),
            Expr::Type(ty) => self.emit_const(program, TyConst::Type(ty.clone())),
            Expr::Decl(decl) => program.code.push(TyInstr::LoadGlobal(decl.clone())),
            Expr::Ref(reference) => {
                if let Some(term) = &reference.term {
                    self.emit_const(program, TyConst::Type(term.clone()));
                } else {
                    program
                        .code
                        .push(TyInstr::LoadGlobal(reference.decl.clone()));
                }
            }
            Expr::Contextual(expr) => self.emit_expr(program, expr),
            Expr::Pattern(pattern) => {
                if let Pattern::Expr(expr) = pattern.as_ref() {
                    self.emit_expr(program, expr);
                } else {
                    self.emit_const(program, TyConst::Any);
                }
            }
            Expr::Import(..)
            | Expr::Include(..)
            | Expr::Element(..)
            | Expr::Show(..)
            | Expr::Set(..)
            | Expr::ContentRef(..)
            | Expr::WhileLoop(..)
            | Expr::ForLoop(..)
            | Expr::Star => self.emit_const(program, TyConst::Any),
        }
    }

    fn emit_block(&mut self, program: &mut TyProgram, exprs: &[Expr]) {
        if let Some((last, prefix)) = exprs.split_last() {
            for expr in prefix {
                self.emit_expr(program, expr);
                program.code.push(TyInstr::Pop);
            }
            self.emit_expr(program, last);
        } else {
            self.emit_const(program, TyConst::None);
        }
    }

    fn emit_array(&mut self, program: &mut TyProgram, args: &ArgsExpr) {
        let mut len = 0;
        for arg in &args.args {
            if let ArgExpr::Pos(expr) = arg {
                self.emit_expr(program, expr);
                len += 1;
            }
        }
        program.code.push(TyInstr::MakeArray { len });
    }

    fn emit_dict(&mut self, program: &mut TyProgram, args: &ArgsExpr) {
        let mut len = 0;
        for arg in &args.args {
            match arg {
                ArgExpr::Named(named) => {
                    self.emit_expr(program, &named.1);
                    len += 1;
                }
                ArgExpr::NamedRt(named) => {
                    self.emit_expr(program, &named.0);
                    self.emit_expr(program, &named.1);
                    len += 1;
                }
                ArgExpr::Spread(expr) => {
                    self.emit_expr(program, expr);
                    len += 1;
                }
                ArgExpr::Pos(..) => {}
            }
        }
        program.code.push(TyInstr::MakeDict { len });
    }

    fn emit_args(&mut self, program: &mut TyProgram, args: &ArgsExpr) {
        let mut positional = 0;
        let mut named = Vec::new();
        let mut spreads = 0;
        for arg in &args.args {
            match arg {
                ArgExpr::Pos(expr) => {
                    self.emit_expr(program, expr);
                    positional += 1;
                }
                ArgExpr::Named(value) => {
                    self.emit_expr(program, &value.1);
                    named.push(value.0.name().clone());
                }
                ArgExpr::NamedRt(value) => {
                    self.emit_expr(program, &value.0);
                    self.emit_expr(program, &value.1);
                    spreads += 1;
                }
                ArgExpr::Spread(expr) => {
                    self.emit_expr(program, expr);
                    spreads += 1;
                }
            }
        }
        program.code.push(TyInstr::MakeArgs(ArgsShape {
            positional,
            named,
            spreads,
        }));
    }

    fn emit_unary(&mut self, program: &mut TyProgram, unary: &UnExpr) {
        self.emit_expr(program, &unary.lhs);
        program.code.push(TyInstr::Unary(unary.op));
    }

    fn emit_binary(&mut self, program: &mut TyProgram, binary: &BinExpr) {
        self.emit_expr(program, &binary.operands.0);
        self.emit_expr(program, &binary.operands.1);
        program.code.push(TyInstr::Binary(binary.op));
    }

    fn emit_apply(&mut self, program: &mut TyProgram, apply: &ApplyExpr) {
        self.emit_expr(program, &apply.callee);
        self.emit_expr(program, &apply.args);
        program.code.push(TyInstr::Call);
    }

    fn emit_func(&mut self, program: &mut TyProgram, func: &FuncExpr) {
        let mut body = TyProgram::default();
        self.emit_expr(&mut body, &func.body);
        body.code.push(TyInstr::Return);

        let ret = self.fresh_meta();
        let proto = ClosureProto {
            decl: func.decl.clone(),
            params: ClosureParams::from_sig(&func.params),
            captures: Vec::new(),
            ret,
            body,
        };
        let id = program.push_closure(proto);
        program.code.push(TyInstr::MakeClosure(id));
    }

    fn emit_select(&mut self, program: &mut TyProgram, select: &SelectExpr) {
        self.emit_expr(program, &select.lhs);
        program
            .code
            .push(TyInstr::Select(select.key.name().clone()));
    }

    fn emit_conditional(&mut self, program: &mut TyProgram, if_expr: &IfExpr) {
        self.emit_expr(program, &if_expr.cond);
        let jump_if_false = program.code.len();
        program.code.push(TyInstr::JumpIfFalse { target: 0 });

        self.emit_expr(program, &if_expr.then);
        let jump_end = program.code.len();
        program.code.push(TyInstr::Jump { target: 0 });

        let else_start = program.code.len() as u32;
        self.emit_expr(program, &if_expr.else_);
        let end = program.code.len() as u32;

        program.code[jump_if_false] = TyInstr::JumpIfFalse { target: else_start };
        program.code[jump_end] = TyInstr::Jump { target: end };
        program.code.push(TyInstr::Join { count: 2 });
    }

    fn emit_const(&mut self, program: &mut TyProgram, constant: TyConst) {
        let id = program.push_const(constant);
        program.code.push(TyInstr::LoadConst(id));
    }

    fn fresh_meta(&mut self) -> MetaId {
        let id = MetaId(self.next_meta);
        self.next_meta += 1;
        id
    }
}

/// WebAssembly host ABI names and handle types for emitted type bytecode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WasmHostAbi {
    /// Module name for imported host functions.
    pub(crate) module: &'static str,
    /// Value handle type.
    pub(crate) value_handle: WasmHandleType,
    /// Args handle type.
    pub(crate) args_handle: WasmHandleType,
    /// Environment handle type.
    pub(crate) env_handle: WasmHandleType,
    /// String handle type.
    pub(crate) string_handle: WasmHandleType,
    /// Meta handle type.
    pub(crate) meta_handle: WasmHandleType,
    /// Closure handle type.
    pub(crate) closure_handle: WasmHandleType,
}

impl Default for WasmHostAbi {
    fn default() -> Self {
        Self {
            module: "tinymist_ty_host",
            value_handle: WasmHandleType::I64,
            args_handle: WasmHandleType::I64,
            env_handle: WasmHandleType::I64,
            string_handle: WasmHandleType::I32,
            meta_handle: WasmHandleType::I32,
            closure_handle: WasmHandleType::I32,
        }
    }
}

/// Numeric wasm handle type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WasmHandleType {
    /// 32-bit integer handle.
    I32,
    /// 64-bit integer handle.
    I64,
}

impl WasmHandleType {
    fn wat(self) -> &'static str {
        match self {
            WasmHandleType::I32 => "i32",
            WasmHandleType::I64 => "i64",
        }
    }
}

/// Experimental wasm emitter for a small bytecode subset.
#[derive(Debug, Default)]
pub(crate) struct WasmEmitter {
    abi: WasmHostAbi,
}

impl WasmEmitter {
    /// Emits a deterministic WAT-shaped module for validation tests.
    pub(crate) fn emit_wat(&self, program: &TyProgram) -> EcoString {
        let mut out = EcoString::new();
        out.push_str("(module\n");
        self.emit_imports(&mut out);
        out.push_str("  (func $run (result ");
        out.push_str(self.abi.value_handle.wat());
        out.push_str(")\n");
        for instr in &program.code {
            self.emit_instr(&mut out, instr);
        }
        out.push_str("  )\n");
        out.push_str(")\n");
        out
    }

    fn emit_imports(&self, out: &mut EcoString) {
        let value = self.abi.value_handle.wat();
        out.push_str("  (import \"");
        out.push_str(self.abi.module);
        out.push_str("\" \"const\" (func $host.const (param i32) (result ");
        out.push_str(value);
        out.push_str(")))\n");
        out.push_str("  (import \"");
        out.push_str(self.abi.module);
        out.push_str("\" \"global\" (func $host.global (param i32) (result ");
        out.push_str(value);
        out.push_str(")))\n");
        out.push_str("  (import \"");
        out.push_str(self.abi.module);
        out.push_str("\" \"call\" (func $host.call (param ");
        out.push_str(value);
        out.push(' ');
        out.push_str(value);
        out.push_str(") (result ");
        out.push_str(value);
        out.push_str(")))\n");
        out.push_str("  (import \"");
        out.push_str(self.abi.module);
        out.push_str("\" \"select\" (func $host.select (param ");
        out.push_str(value);
        out.push_str(" i32) (result ");
        out.push_str(value);
        out.push_str(")))\n");
    }

    fn emit_instr(&self, out: &mut EcoString, instr: &TyInstr) {
        match instr {
            TyInstr::LoadConst(id) => {
                out.push_str("    i32.const ");
                out.push_str(&id.0.to_string());
                out.push_str("\n    call $host.const\n");
            }
            TyInstr::LoadGlobal(..) => {
                out.push_str("    i32.const 0\n    call $host.global\n");
            }
            TyInstr::Call => out.push_str("    call $host.call\n"),
            TyInstr::Select(..) => out.push_str("    i32.const 0\n    call $host.select\n"),
            TyInstr::Return => {}
            _ => out.push_str("    ;; unsupported type-bytecode instruction\n"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{syntax::def::Decl, ty::BuiltinTy};
    use typst::syntax::Span;

    fn decl(name: &str) -> DeclExpr {
        Interned::new(Decl::lit(name))
    }

    fn var(name: &str) -> Expr {
        Expr::Decl(decl(name))
    }

    fn ty(ty: Ty) -> Expr {
        Expr::Type(ty)
    }

    #[test]
    fn compile_call_and_select() {
        let expr = Expr::Apply(Interned::new(ApplyExpr {
            callee: Expr::Select(SelectExpr::new(decl("field"), var("obj"))),
            args: Expr::Args(ArgsExpr::new(
                Span::detached(),
                vec![ArgExpr::Pos(ty(Ty::Builtin(BuiltinTy::None)))],
            )),
            span: Span::detached(),
        }));

        let program = TyBytecodeCompiler::default().compile_expr(&expr);
        assert_eq!(
            format!("{:?}", program.code),
            "[LoadGlobal(Var(obj)), Select(\"field\"), LoadConst(ConstId(0)), MakeArgs(ArgsShape { positional: 1, named: [], spreads: 0 }), Call, Return]"
        );
    }

    #[test]
    fn compile_binary_and_conditional() {
        let expr = Expr::Conditional(Interned::new(IfExpr {
            cond: var("cond"),
            then: Expr::Binary(BinExpr::new(
                BinOp::Add,
                ty(Ty::Builtin(BuiltinTy::None)),
                ty(Ty::Builtin(BuiltinTy::None)),
            )),
            else_: ty(Ty::Builtin(BuiltinTy::Auto)),
        }));

        let program = TyBytecodeCompiler::default().compile_expr(&expr);
        assert_eq!(
            format!("{:?}", program.code),
            "[LoadGlobal(Var(cond)), JumpIfFalse { target: 6 }, LoadConst(ConstId(0)), LoadConst(ConstId(1)), Binary(Add), Jump { target: 7 }, LoadConst(ConstId(2)), Join { count: 2 }, Return]"
        );
    }

    #[test]
    fn compile_recursive_function_proto() {
        let f = decl("f");
        let expr = Expr::Func(Interned::new(FuncExpr {
            decl: f.clone(),
            params: PatternSig {
                pos: Default::default(),
                named: Default::default(),
                spread_left: None,
                spread_right: None,
            },
            body: Expr::Apply(Interned::new(ApplyExpr {
                callee: Expr::Decl(f),
                args: Expr::Args(ArgsExpr::new(Span::detached(), vec![])),
                span: Span::detached(),
            })),
        }));

        let program = TyBytecodeCompiler::default().compile_expr(&expr);
        assert_eq!(program.closures.len(), 1);
        assert_eq!(
            format!("{:?}", program.closures[0].body.code),
            "[LoadGlobal(Var(f)), MakeArgs(ArgsShape { positional: 0, named: [], spreads: 0 }), Call, Return]"
        );
    }

    #[test]
    fn quote_neutral_values() {
        let field = Interned::new_str("body");
        let value = SemValue::Neutral(NeutralValue::Select {
            value: Box::new(SemValue::Type(Ty::Builtin(BuiltinTy::Content(None)))),
            field: field.clone(),
        });

        assert_eq!(format!("{:?}", value.quote_ty()), "Content.body");
    }

    #[test]
    fn emit_wasm_contract_for_supported_subset() {
        let expr = Expr::Apply(Interned::new(ApplyExpr {
            callee: var("f"),
            args: Expr::Args(ArgsExpr::new(Span::detached(), vec![])),
            span: Span::detached(),
        }));
        let program = TyBytecodeCompiler::default().compile_expr(&expr);
        let wat = WasmEmitter::default().emit_wat(&program);

        assert!(wat.contains("(import \"tinymist_ty_host\" \"const\""));
        assert!(wat.contains("(import \"tinymist_ty_host\" \"call\""));
        assert!(wat.contains("call $host.global"));
        assert!(wat.contains("call $host.call"));
    }
}
