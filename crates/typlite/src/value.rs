//! # Typlite Values

use core::fmt;

use crate::*;

pub type RawFunc = fn(Args) -> Result<Value>;

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
pub enum Value {
    None,
    RawFunc(RawFunc),
    Str(EcoString),
    Content(EcoString),
    Image { path: EcoString, alt: EcoString },
}

impl From<RawFunc> for Value {
    fn from(func: RawFunc) -> Self {
        Self::RawFunc(func)
    }
}

pub struct Content(pub EcoString);

impl fmt::Display for Content {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub struct LazyContent(pub EcoString);

impl fmt::Display for LazyContent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub struct Args<'a> {
    pub vm: &'a mut TypliteWorker,
    pub args: ast::Args<'a>,
    pub pos: Vec<&'a SyntaxNode>,
}

impl<'a> Args<'a> {
    pub fn new(worker: &'a mut TypliteWorker, args: ast::Args<'a>) -> Self {
        let pos = args
            .items()
            .filter_map(|item| match item {
                ast::Arg::Pos(pos) => Some(pos.to_untyped()),
                _ => None,
            })
            .rev()
            .collect();
        Self {
            vm: worker,
            args,
            pos,
        }
    }

    pub fn get_named_(&mut self, key: &str) -> Option<&'a SyntaxNode> {
        // find named
        for item in self.args.items() {
            if let ast::Arg::Named(named) = item {
                if named.name().get() == key {
                    return Some(named.expr().to_untyped());
                }
            }
        }

        None
    }

    pub fn get(&mut self, key: &str) -> Result<&'a SyntaxNode> {
        if let Some(named) = self.get_named_(key) {
            return Ok(named);
        }

        // find positional
        Ok(self
            .pos
            .pop()
            .ok_or_else(|| format!("missing positional arguments: {key}"))?)
    }

    pub fn parse<T: Eval<'a>>(&mut self, node: &'a SyntaxNode) -> Result<T> {
        T::eval(node, self.vm)
    }
}

#[macro_export]
macro_rules! get_pos_named {
    (
        $args:expr,
        $key:ident: $ty:ty
    ) => {{
        let raw = $args.get(stringify!($key))?;
        $args.parse::<$ty>(raw)?
    }};
}
pub use get_pos_named;

#[macro_export]
macro_rules! get_named {
    (
        $args:expr,
        $key:ident: Option<$ty:ty>
    ) => {{
        if let Some(raw) = $args.get_named_(stringify!($key)) {
            Some($args.parse::<$ty>(raw)?)
        } else {
            None
        }
    }};
    (
        $args:expr,
        $key:ident: $ty:ty
    ) => {{
        let raw = $args.get_named(stringify!($key))?;
        $args.parse::<$ty>(raw)?
    }};
    (
        $args:expr,
        $key:ident: $ty:ty := $default:expr
    ) => {{
        if let Some(raw) = $args.get_named_(stringify!($key)) {
            $args.parse::<$ty>(raw)?
        } else {
            $default.into()
        }
    }};
}
pub use get_named;

/// Evaluate an expression.
pub trait Eval<'a>: Sized {
    /// Evaluate the expression to the output value.
    fn eval(node: &'a SyntaxNode, vm: &mut TypliteWorker) -> Result<Self>;
}

impl<'a> Eval<'a> for () {
    fn eval(_node: &'a SyntaxNode, _vm: &mut TypliteWorker) -> Result<Self> {
        Ok(())
    }
}

impl<'a> Eval<'a> for &'a SyntaxNode {
    fn eval(node: &'a SyntaxNode, _vm: &mut TypliteWorker) -> Result<Self> {
        Ok(node)
    }
}

impl<'a> Eval<'a> for EcoString {
    fn eval(node: &'a SyntaxNode, _vm: &mut TypliteWorker) -> Result<Self> {
        let node: ast::Str = node
            .cast()
            .ok_or_else(|| format!("expected string, found {:?}", node.kind()))?;
        Ok(node.get())
    }
}

impl<'a> Eval<'a> for Value {
    fn eval(node: &'a SyntaxNode, vm: &mut TypliteWorker) -> Result<Self> {
        vm.eval(node)
    }
}

impl<'a> Eval<'a> for Content {
    fn eval(node: &'a SyntaxNode, vm: &mut TypliteWorker) -> Result<Self> {
        Ok(Self(vm.convert(node)?))
    }
}

impl<'a> Eval<'a> for LazyContent {
    fn eval(node: &'a SyntaxNode, vm: &mut TypliteWorker) -> Result<Self> {
        let node = match node.cast() {
            Some(s @ ast::Closure { .. }) => s.body().to_untyped(),
            None => node,
        };

        Ok(Self(vm.convert(node)?))
    }
}
