use crate::*;

pub type RawFunc = fn(ArgGetter) -> Result<Value>;

#[derive(Debug)]
pub enum Value {
    RawFunc(RawFunc),
    Content(EcoString),
}

impl From<RawFunc> for Value {
    fn from(func: RawFunc) -> Self {
        Self::RawFunc(func)
    }
}

pub struct ArgGetter<'a> {
    pub worker: &'a mut TypliteWorker,
    pub args: ast::Args<'a>,
    pub pos: Vec<&'a SyntaxNode>,
}

impl<'a> ArgGetter<'a> {
    pub fn new(worker: &'a mut TypliteWorker, args: ast::Args<'a>) -> Self {
        let pos = args
            .items()
            .filter_map(|item| match item {
                ast::Arg::Pos(pos) => Some(pos.to_untyped()),
                _ => None,
            })
            .rev()
            .collect();
        Self { worker, args, pos }
    }

    pub fn get(&mut self, key: &str) -> Result<&'a SyntaxNode> {
        // find named
        for item in self.args.items() {
            if let ast::Arg::Named(named) = item {
                if named.name().get() == key {
                    return Ok(named.expr().to_untyped());
                }
            }
        }

        // find positional
        Ok(self
            .pos
            .pop()
            .ok_or_else(|| format!("missing positional arguments: {key}"))?)
    }

    pub fn parse<T: Eval<'a>>(&mut self, node: &'a SyntaxNode) -> Result<T> {
        T::eval(node, self.worker)
    }
}

// [attr] key: ty
macro_rules! get_pos_named {
    (
        $args:expr,
        $key:ident: $ty:ty
    ) => {{
        let raw = $args.get(stringify!($key))?;
        $args.parse::<$ty>(raw)?
    }};
}
pub(crate) use get_pos_named;

/// Evaluate an expression.
pub trait Eval<'a>: Sized {
    /// Evaluate the expression to the output value.
    fn eval(node: &'a SyntaxNode, vm: &mut TypliteWorker) -> Result<Self>;
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
