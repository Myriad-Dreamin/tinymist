//! Experimental bytecode model for type-level evaluation.
//!
//! This module is intentionally not wired into the checker yet. It defines the
//! compact program model used by later type VM work while preserving `Ty` as the
//! external representation.

#![allow(dead_code)]

use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    fmt::Debug,
    hash::{Hash, Hasher},
    sync::Arc,
};

use dashmap::DashMap;
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
    /// Prototype body and metadata.
    pub(crate) data: Arc<ClosureProto>,
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

/// Whether the checker may safely route this expression through the bytecode
/// deduce path without losing side effects from the legacy traversal.
pub(crate) fn supports_compile_before_check(expr: &Expr) -> bool {
    match expr {
        Expr::Array(args) | Expr::Dict(args) | Expr::Args(args) => {
            args.args.iter().all(|arg| match arg {
                ArgExpr::Pos(expr) | ArgExpr::Spread(expr) => supports_compile_before_check(expr),
                ArgExpr::Named(named) => supports_compile_before_check(&named.1),
                ArgExpr::NamedRt(named) => {
                    supports_compile_before_check(&named.0)
                        && supports_compile_before_check(&named.1)
                }
            })
        }
        Expr::Unary(unary) => supports_compile_before_check(&unary.lhs),
        Expr::Binary(binary) => {
            supports_compile_before_check(&binary.operands.0)
                && supports_compile_before_check(&binary.operands.1)
        }
        Expr::Apply(apply) => {
            supports_compile_before_check(&apply.callee)
                && supports_compile_before_check(&apply.args)
        }
        Expr::Select(select) => supports_compile_before_check(&select.lhs),
        Expr::Conditional(if_expr) => {
            supports_compile_before_check(&if_expr.cond)
                && supports_compile_before_check(&if_expr.then)
                && supports_compile_before_check(&if_expr.else_)
        }
        Expr::Contextual(expr) => supports_compile_before_check(expr),
        Expr::Type(..) | Expr::Decl(..) | Expr::Ref(..) => true,
        Expr::Block(..)
        | Expr::Func(..)
        | Expr::Let(..)
        | Expr::Pattern(..)
        | Expr::Element(..)
        | Expr::Import(..)
        | Expr::Include(..)
        | Expr::Show(..)
        | Expr::Set(..)
        | Expr::ContentRef(..)
        | Expr::WhileLoop(..)
        | Expr::ForLoop(..)
        | Expr::Star => false,
    }
}

/// Whether the checker may use VM evaluation directly without needing lexical
/// bindings from the legacy `TypeInfo` scope.
pub(crate) fn supports_binding_free_check(expr: &Expr) -> bool {
    match expr {
        Expr::Array(args) | Expr::Dict(args) | Expr::Args(args) => {
            args.args.iter().all(|arg| match arg {
                ArgExpr::Pos(expr) | ArgExpr::Spread(expr) => supports_binding_free_check(expr),
                ArgExpr::Named(named) => supports_binding_free_check(&named.1),
                ArgExpr::NamedRt(named) => {
                    supports_binding_free_check(&named.0) && supports_binding_free_check(&named.1)
                }
            })
        }
        Expr::Unary(unary) => supports_binding_free_check(&unary.lhs),
        Expr::Binary(binary) => {
            supports_binding_free_check(&binary.operands.0)
                && supports_binding_free_check(&binary.operands.1)
        }
        Expr::Contextual(expr) => supports_binding_free_check(expr),
        Expr::Type(..) => true,
        Expr::Decl(..)
        | Expr::Ref(..)
        | Expr::Apply(..)
        | Expr::Select(..)
        | Expr::Conditional(..)
        | Expr::Block(..)
        | Expr::Func(..)
        | Expr::Let(..)
        | Expr::Pattern(..)
        | Expr::Element(..)
        | Expr::Import(..)
        | Expr::Include(..)
        | Expr::Show(..)
        | Expr::Set(..)
        | Expr::ContentRef(..)
        | Expr::WhileLoop(..)
        | Expr::ForLoop(..)
        | Expr::Star => false,
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

/// Execution backend for type bytecode programs.
pub(crate) trait TyExecutionBackend {
    /// Execute a bytecode program.
    fn execute(
        &self,
        program: &TyProgram,
        env: &ExecutionEnv,
        caches: &TyVmCaches,
    ) -> ExecutionResult;
}

/// Result of a bytecode execution request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ExecutionResult {
    /// Computed semantic value.
    pub(crate) value: SemValue,
    /// Metrics collected while evaluating.
    pub(crate) metrics: VmMetrics,
}

/// Rust interpreter backend.
#[derive(Debug, Default)]
pub(crate) struct RustInterpreterBackend;

impl TyExecutionBackend for RustInterpreterBackend {
    fn execute(
        &self,
        program: &TyProgram,
        env: &ExecutionEnv,
        caches: &TyVmCaches,
    ) -> ExecutionResult {
        let key = ProgramCacheKey::new(program, env);
        if let Some(value) = caches.programs.get(&key) {
            return ExecutionResult {
                value: value.clone(),
                metrics: VmMetrics {
                    cache_hits: 1,
                    ..Default::default()
                },
            };
        }

        let mut vm = TyVm::new(caches, env.clone());
        vm.metrics.cache_misses += 1;
        let value = vm.eval_program(program);
        caches.programs.insert(key, value.clone());
        ExecutionResult {
            value,
            metrics: vm.metrics,
        }
    }
}

/// Execution environment and cache invalidation context.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct ExecutionEnv {
    /// Global values visible to bytecode.
    pub(crate) globals: HashMap<DeclExpr, SemValue>,
    /// Source revision component.
    pub(crate) source_revision: u64,
    /// Context component.
    pub(crate) context_key: u64,
    /// Meta epoch component.
    pub(crate) meta_epoch: u64,
}

impl ExecutionEnv {
    fn fingerprint(&self) -> u64 {
        fingerprint(&(
            self.source_revision,
            self.context_key,
            self.meta_epoch,
            stable_debug(&self.globals),
        ))
    }
}

/// Global completed-result caches for the type VM.
#[derive(Debug, Default)]
pub(crate) struct TyVmCaches {
    /// Completed top-level program results.
    pub(crate) programs: DashMap<ProgramCacheKey, SemValue>,
    /// Completed closure-call results.
    pub(crate) closure_calls: DashMap<ClosureCallKey, SemValue>,
    /// Completed quote results.
    pub(crate) quotes: DashMap<QuoteCacheKey, Ty>,
}

/// Cache key for top-level program execution.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ProgramCacheKey {
    program: u64,
    source_revision: u64,
    context_key: u64,
    meta_epoch: u64,
    env: u64,
}

impl ProgramCacheKey {
    fn new(program: &TyProgram, env: &ExecutionEnv) -> Self {
        Self {
            program: fingerprint(program),
            source_revision: env.source_revision,
            context_key: env.context_key,
            meta_epoch: env.meta_epoch,
            env: env.fingerprint(),
        }
    }
}

/// Cache key for closure calls.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct ClosureCallKey {
    proto: ClosureProtoId,
    body: u64,
    captures: u64,
    args: u64,
    source_revision: u64,
    context_key: u64,
    meta_epoch: u64,
}

impl ClosureCallKey {
    fn new(closure: &ClosureValue, args: &SemArgs, env: &ExecutionEnv) -> Self {
        Self {
            proto: closure.proto,
            body: fingerprint(&closure.data.body),
            captures: fingerprint(&stable_debug(&closure.captures)),
            args: fingerprint(&stable_debug(args)),
            source_revision: env.source_revision,
            context_key: env.context_key,
            meta_epoch: env.meta_epoch,
        }
    }
}

/// Cache key for quote results.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct QuoteCacheKey {
    value: u64,
    meta_epoch: u64,
}

impl QuoteCacheKey {
    fn new(value: &SemValue, meta_epoch: u64) -> Self {
        Self {
            value: fingerprint(&stable_debug(value)),
            meta_epoch,
        }
    }
}

/// Local closure-call execution state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ClosureCallState {
    /// A call has not started in this VM.
    Fresh,
    /// A call is currently running in this VM.
    Running,
    /// A call completed in this VM.
    Done(SemValue),
    /// A call residualized or otherwise got stuck.
    Stuck(SemValue),
}

/// VM counters used by package-scale validation.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VmMetrics {
    /// Interpreted instruction count.
    pub(crate) steps: u64,
    /// Completed-result cache hits.
    pub(crate) cache_hits: u64,
    /// Completed-result cache misses.
    pub(crate) cache_misses: u64,
    /// Recursive calls converted to neutral residuals.
    pub(crate) residualized_cycles: u64,
    /// Wasmer compile time in nanoseconds.
    pub(crate) wasmer_compile_ns: u64,
    /// Wasmer run time in nanoseconds.
    pub(crate) wasmer_run_ns: u64,
}

impl std::ops::AddAssign for VmMetrics {
    fn add_assign(&mut self, rhs: Self) {
        self.steps += rhs.steps;
        self.cache_hits += rhs.cache_hits;
        self.cache_misses += rhs.cache_misses;
        self.residualized_cycles += rhs.residualized_cycles;
        self.wasmer_compile_ns += rhs.wasmer_compile_ns;
        self.wasmer_run_ns += rhs.wasmer_run_ns;
    }
}

/// Pretty package-scan metrics payload. Callers can write this under `target/`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PackageVmMetrics {
    /// Package identifier.
    pub(crate) package: EcoString,
    /// Collected VM metrics.
    pub(crate) metrics: VmMetrics,
}

impl PackageVmMetrics {
    /// Deterministic text format for package scan output under `target/`.
    pub(crate) fn to_pretty_text(&self) -> EcoString {
        let mut out = EcoString::new();
        out.push_str("package: ");
        out.push_str(&self.package);
        out.push('\n');
        out.push_str("steps: ");
        out.push_str(&self.metrics.steps.to_string());
        out.push('\n');
        out.push_str("cache_hits: ");
        out.push_str(&self.metrics.cache_hits.to_string());
        out.push('\n');
        out.push_str("cache_misses: ");
        out.push_str(&self.metrics.cache_misses.to_string());
        out.push('\n');
        out.push_str("residualized_cycles: ");
        out.push_str(&self.metrics.residualized_cycles.to_string());
        out.push('\n');
        out.push_str("wasmer_compile_ns: ");
        out.push_str(&self.metrics.wasmer_compile_ns.to_string());
        out.push('\n');
        out.push_str("wasmer_run_ns: ");
        out.push_str(&self.metrics.wasmer_run_ns.to_string());
        out.push('\n');
        out
    }
}

struct TyVm<'a> {
    caches: &'a TyVmCaches,
    env: ExecutionEnv,
    stack: Vec<SemValue>,
    locals: Vec<SemValue>,
    call_states: HashMap<ClosureCallKey, ClosureCallState>,
    metrics: VmMetrics,
}

impl<'a> TyVm<'a> {
    fn new(caches: &'a TyVmCaches, env: ExecutionEnv) -> Self {
        Self {
            caches,
            env,
            stack: Vec::new(),
            locals: Vec::new(),
            call_states: HashMap::new(),
            metrics: VmMetrics::default(),
        }
    }

    fn eval_program(&mut self, program: &TyProgram) -> SemValue {
        self.eval_code(program).unwrap_or(SemValue::Any)
    }

    fn eval_code(&mut self, program: &TyProgram) -> Option<SemValue> {
        let mut pc = 0usize;
        while let Some(instr) = program.code.get(pc) {
            self.metrics.steps += 1;
            match instr {
                TyInstr::LoadConst(id) => self.stack.push(self.load_const(program, *id)),
                TyInstr::LoadLocal(id) => self.stack.push(
                    self.locals
                        .get(id.0 as usize)
                        .cloned()
                        .unwrap_or_else(|| SemValue::Neutral(NeutralValue::Local(*id))),
                ),
                TyInstr::LoadGlobal(decl) => {
                    self.stack
                        .push(self.env.globals.get(decl).cloned().unwrap_or_else(|| {
                            SemValue::Neutral(NeutralValue::Global {
                                decl: decl.clone(),
                                known: None,
                            })
                        }))
                }
                TyInstr::StoreLocal(id) => {
                    let value = self.stack.pop().unwrap_or(SemValue::Any);
                    let slot = id.0 as usize;
                    if self.locals.len() <= slot {
                        self.locals.resize(slot + 1, SemValue::Any);
                    }
                    self.locals[slot] = value;
                }
                TyInstr::LoadCapture(id) => self
                    .stack
                    .push(SemValue::Neutral(NeutralValue::Local(LocalId(*id)))),
                TyInstr::Pop => {
                    self.stack.pop();
                }
                TyInstr::MakeArgs(shape) => self.make_args(shape),
                TyInstr::MakeArray { len } => {
                    let values = self.pop_many(*len as usize);
                    self.stack.push(SemValue::Array(values));
                }
                TyInstr::MakeDict { len } => {
                    let values = self.pop_many(*len as usize);
                    let fields = values
                        .into_iter()
                        .enumerate()
                        .map(|(idx, value)| (Interned::new_str(&idx.to_string()), value))
                        .collect();
                    self.stack.push(SemValue::Record(fields));
                }
                TyInstr::MakeTuple { len } => {
                    let values = self.pop_many(*len as usize);
                    self.stack.push(SemValue::Tuple(values));
                }
                TyInstr::MakeClosure(id) => {
                    let Some(proto) = program.closures.get(id.0 as usize) else {
                        self.stack.push(SemValue::Any);
                        pc += 1;
                        continue;
                    };
                    self.stack.push(SemValue::Closure(ClosureValue {
                        proto: *id,
                        data: Arc::new(proto.clone()),
                        captures: Vec::new(),
                    }));
                }
                TyInstr::Call => self.call(),
                TyInstr::Select(field) => {
                    let value = self.stack.pop().unwrap_or(SemValue::Any);
                    let selected = self.select(value, field.clone());
                    self.stack.push(selected);
                }
                TyInstr::Unary(op) => {
                    let value = self.stack.pop().unwrap_or(SemValue::Any);
                    self.stack.push(SemValue::Neutral(NeutralValue::Unary {
                        op: *op,
                        value: Box::new(value),
                    }));
                }
                TyInstr::Binary(op) => {
                    let rhs = self.stack.pop().unwrap_or(SemValue::Any);
                    let lhs = self.stack.pop().unwrap_or(SemValue::Any);
                    let value = self.binary(*op, lhs, rhs);
                    self.stack.push(value);
                }
                TyInstr::JumpIfFalse { target } => {
                    let cond = self.stack.pop().unwrap_or(SemValue::Any);
                    if matches!(cond, SemValue::Type(Ty::Boolean(Some(false)))) {
                        pc = *target as usize;
                        continue;
                    }
                }
                TyInstr::Jump { target } => {
                    pc = *target as usize;
                    continue;
                }
                TyInstr::Join { count } => {
                    let values = self.pop_many(*count as usize);
                    let ty = Ty::from_types(values.iter().map(QuoteTy::quote_ty));
                    self.stack.push(SemValue::Type(ty));
                }
                TyInstr::Return => return self.stack.pop(),
            }
            pc += 1;
        }

        self.stack.pop()
    }

    fn load_const(&self, program: &TyProgram, id: ConstId) -> SemValue {
        match program.consts.get(id.0 as usize) {
            Some(TyConst::Type(ty)) => SemValue::Type(ty.clone()),
            Some(TyConst::Str(field)) => SemValue::Type(Ty::Value(crate::ty::InsTy::new(
                typst::foundations::Value::Str(field.as_ref().into()),
            ))),
            Some(TyConst::None) => SemValue::None,
            Some(TyConst::Any) | None => SemValue::Any,
        }
    }

    fn make_args(&mut self, shape: &ArgsShape) {
        let spreads = self.pop_many(shape.spreads as usize);
        let named_values = self.pop_many(shape.named.len());
        let positional = self.pop_many(shape.positional as usize);
        let named = shape.named.iter().cloned().zip(named_values).collect();
        self.stack.push(SemValue::Args(SemArgs {
            positional,
            named,
            spreads,
        }));
    }

    fn call(&mut self) {
        let args = match self.stack.pop().unwrap_or(SemValue::Any) {
            SemValue::Args(args) => args,
            value => SemArgs {
                positional: vec![value],
                named: Vec::new(),
                spreads: Vec::new(),
            },
        };
        let callee = self.stack.pop().unwrap_or(SemValue::Any);
        let value = match callee {
            SemValue::Closure(closure) => self.call_closure(closure, args),
            callee => SemValue::Neutral(NeutralValue::Apply {
                callee: Box::new(callee),
                args,
            }),
        };
        self.stack.push(value);
    }

    fn call_closure(&mut self, closure: ClosureValue, args: SemArgs) -> SemValue {
        let key = ClosureCallKey::new(&closure, &args, &self.env);

        match self
            .call_states
            .get(&key)
            .cloned()
            .unwrap_or(ClosureCallState::Fresh)
        {
            ClosureCallState::Running => {
                self.metrics.residualized_cycles += 1;
                return SemValue::Neutral(NeutralValue::Apply {
                    callee: Box::new(SemValue::Closure(closure)),
                    args,
                });
            }
            ClosureCallState::Done(value) | ClosureCallState::Stuck(value) => return value,
            ClosureCallState::Fresh => {}
        }

        if let Some(value) = self.caches.closure_calls.get(&key) {
            self.metrics.cache_hits += 1;
            return value.clone();
        }

        self.metrics.cache_misses += 1;
        self.call_states
            .insert(key.clone(), ClosureCallState::Running);

        let mut child_env = self.env.clone();
        child_env.globals.insert(
            closure.data.decl.clone(),
            SemValue::Closure(closure.clone()),
        );

        let mut child = TyVm {
            caches: self.caches,
            env: child_env,
            stack: Vec::new(),
            locals: args.positional.clone(),
            call_states: self.call_states.clone(),
            metrics: VmMetrics::default(),
        };
        let value = child.eval_program(&closure.data.body);
        self.metrics += child.metrics;

        let state = if matches!(value, SemValue::Neutral(..)) {
            ClosureCallState::Stuck(value.clone())
        } else {
            ClosureCallState::Done(value.clone())
        };
        self.call_states.insert(key.clone(), state);
        if !matches!(value, SemValue::Neutral(..)) {
            self.caches.closure_calls.insert(key, value.clone());
        }
        value
    }

    fn select(&mut self, value: SemValue, field: StrRef) -> SemValue {
        match value {
            SemValue::Record(fields) => fields
                .into_iter()
                .find_map(|(name, value)| (name == field).then_some(value))
                .unwrap_or(SemValue::Any),
            value => SemValue::Neutral(NeutralValue::Select {
                value: Box::new(value),
                field,
            }),
        }
    }

    fn binary(&mut self, op: BinOp, lhs: SemValue, rhs: SemValue) -> SemValue {
        match (&lhs, &rhs) {
            (SemValue::Type(Ty::Value(lhs_val)), SemValue::Type(Ty::Value(rhs_val)))
                if op == BinOp::Add =>
            {
                if let (
                    typst::foundations::Value::Str(lhs_str),
                    typst::foundations::Value::Str(rhs_str),
                ) = (&lhs_val.val, &rhs_val.val)
                {
                    let mut combined = EcoString::with_capacity(lhs_str.len() + rhs_str.len());
                    combined.push_str(lhs_str.as_str());
                    combined.push_str(rhs_str.as_str());
                    return SemValue::Type(Ty::Value(crate::ty::InsTy::new(
                        typst::foundations::Value::Str(combined.into()),
                    )));
                }
            }
            (SemValue::Type(lhs), SemValue::Type(rhs)) => {
                return SemValue::Type(Ty::Binary(TypeBinary::new(op, lhs.clone(), rhs.clone())));
            }
            _ => {}
        }

        SemValue::Neutral(NeutralValue::Binary {
            op,
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
        })
    }

    fn pop_many(&mut self, len: usize) -> Vec<SemValue> {
        let keep = self.stack.len().saturating_sub(len);
        self.stack.split_off(keep)
    }
}

impl TyVmCaches {
    /// Quote a semantic value, reusing completed quote results.
    pub(crate) fn quote_cached(&self, value: &SemValue, meta_epoch: u64) -> Ty {
        let key = QuoteCacheKey::new(value, meta_epoch);
        if let Some(quoted) = self.quotes.get(&key) {
            return quoted.clone();
        }
        let quoted = value.quote_ty();
        self.quotes.insert(key, quoted.clone());
        quoted
    }
}

/// Experimental Wasmer-backed executor. This is feature-gated and delegates to
/// the Rust interpreter until wasm instantiation is wired to host handles.
#[cfg(feature = "experimental-wasmer")]
#[derive(Debug, Default)]
pub(crate) struct WasmerBackend {
    interpreter: RustInterpreterBackend,
}

/// Feature-gated Wasmer module contract. The first execution-cache phase keeps
/// host handles opaque and does not expose Rust `Ty` layout to wasm memory.
#[cfg(feature = "experimental-wasmer")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WasmerModuleInstance {
    /// Host ABI used by the module.
    pub(crate) abi: WasmHostAbi,
    /// WAT-shaped module emitted from bytecode.
    pub(crate) wat: EcoString,
    _store: std::marker::PhantomData<fn() -> wasmer::Store>,
}

#[cfg(feature = "experimental-wasmer")]
impl WasmerBackend {
    /// Instantiate the current handle-based wasm contract.
    pub(crate) fn instantiate_contract(&self, program: &TyProgram) -> WasmerModuleInstance {
        let emitter = WasmEmitter::default();
        WasmerModuleInstance {
            abi: emitter.abi.clone(),
            wat: emitter.emit_wat(program),
            _store: std::marker::PhantomData,
        }
    }
}

#[cfg(feature = "experimental-wasmer")]
impl TyExecutionBackend for WasmerBackend {
    fn execute(
        &self,
        program: &TyProgram,
        env: &ExecutionEnv,
        caches: &TyVmCaches,
    ) -> ExecutionResult {
        let compile_started = tinymist_std::time::Instant::now();
        let _module = self.instantiate_contract(program);
        let compile_ns = compile_started.elapsed().as_nanos() as u64;

        let run_started = tinymist_std::time::Instant::now();
        let mut result = self.interpreter.execute(program, env, caches);
        result.metrics.wasmer_compile_ns = compile_ns;
        result.metrics.wasmer_run_ns = run_started.elapsed().as_nanos() as u64;
        result
    }
}

fn stable_debug(value: &impl Debug) -> EcoString {
    EcoString::from(format!("{value:?}"))
}

fn fingerprint(value: &impl Debug) -> u64 {
    let mut hasher = DefaultHasher::new();
    stable_debug(value).hash(&mut hasher);
    hasher.finish()
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

    #[test]
    fn interpreter_executes_supported_call_and_reuses_completed_cache() {
        let f = decl("f");
        let body_ty = Ty::Builtin(BuiltinTy::None);
        let func = Expr::Func(Interned::new(FuncExpr {
            decl: f,
            params: PatternSig {
                pos: Default::default(),
                named: Default::default(),
                spread_left: None,
                spread_right: None,
            },
            body: ty(body_ty.clone()),
        }));
        let compiled = TyBytecodeCompiler::default().compile_expr(&func);
        let mut program = TyProgram::default();
        program.closures = compiled.closures;
        program.code.push(TyInstr::MakeClosure(ClosureProtoId(0)));
        program.code.push(TyInstr::MakeArgs(ArgsShape {
            positional: 0,
            named: Vec::new(),
            spreads: 0,
        }));
        program.code.push(TyInstr::Call);
        program.code.push(TyInstr::Return);

        let caches = TyVmCaches::default();
        let backend = RustInterpreterBackend;
        let env = ExecutionEnv::default();

        let first = backend.execute(&program, &env, &caches);
        assert_eq!(first.value, SemValue::Type(body_ty));
        assert_eq!(first.metrics.cache_misses, 2);
        assert_eq!(caches.programs.len(), 1);
        assert_eq!(caches.closure_calls.len(), 1);

        let second = backend.execute(&program, &env, &caches);
        assert_eq!(second.value, first.value);
        assert_eq!(second.metrics.cache_hits, 1);
    }

    #[test]
    fn interpreter_residualizes_recursive_running_call() {
        let f = decl("f");
        let func = Expr::Func(Interned::new(FuncExpr {
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
        let compiled = TyBytecodeCompiler::default().compile_expr(&func);
        let mut program = TyProgram::default();
        program.closures = compiled.closures;
        program.code.push(TyInstr::MakeClosure(ClosureProtoId(0)));
        program.code.push(TyInstr::MakeArgs(ArgsShape {
            positional: 0,
            named: Vec::new(),
            spreads: 0,
        }));
        program.code.push(TyInstr::Call);
        program.code.push(TyInstr::Return);

        let result = RustInterpreterBackend.execute(
            &program,
            &ExecutionEnv::default(),
            &TyVmCaches::default(),
        );
        assert!(matches!(
            result.value,
            SemValue::Neutral(NeutralValue::Apply { .. })
        ));
        assert_eq!(result.metrics.residualized_cycles, 1);
    }

    #[test]
    fn quote_cache_reuses_completed_quote() {
        let caches = TyVmCaches::default();
        let value = SemValue::Type(Ty::Builtin(BuiltinTy::Auto));
        let first = caches.quote_cached(&value, 7);
        let second = caches.quote_cached(&value, 7);

        assert_eq!(first, second);
        assert_eq!(caches.quotes.len(), 1);
    }

    #[test]
    fn package_vm_metrics_pretty_format_is_target_friendly() {
        let metrics = PackageVmMetrics {
            package: "preview/foo:0.1.0".into(),
            metrics: VmMetrics {
                steps: 11,
                cache_hits: 2,
                cache_misses: 3,
                residualized_cycles: 1,
                wasmer_compile_ns: 5,
                wasmer_run_ns: 7,
            },
        };

        assert_eq!(
            metrics.to_pretty_text().as_str(),
            "package: preview/foo:0.1.0\nsteps: 11\ncache_hits: 2\ncache_misses: 3\nresidualized_cycles: 1\nwasmer_compile_ns: 5\nwasmer_run_ns: 7\n"
        );
    }

    #[cfg(feature = "experimental-wasmer")]
    #[test]
    fn wasmer_backend_matches_interpreter_for_supported_subset() {
        let program = TyBytecodeCompiler::default().compile_expr(&ty(Ty::Builtin(BuiltinTy::Auto)));
        let env = ExecutionEnv::default();

        let interpreted = RustInterpreterBackend.execute(&program, &env, &TyVmCaches::default());
        let wasmer = WasmerBackend::default().execute(&program, &env, &TyVmCaches::default());

        assert_eq!(wasmer.value, interpreted.value);
        assert_eq!(wasmer.metrics.residualized_cycles, 0);
        assert!(
            WasmerBackend::default()
                .instantiate_contract(&program)
                .wat
                .contains("tinymist_ty_host")
        );
    }
}
