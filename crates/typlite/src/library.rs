use super::*;
use ecow::eco_format;
use typst_syntax::ast;
use value::RawFunc;

pub fn library() -> Scopes<Value> {
    let mut scopes = Scopes::new();
    scopes.define("link", link as RawFunc);
    scopes
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

    fn get(&mut self, key: &str) -> Result<&'a SyntaxNode> {
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

    fn parse<T: Eval<'a>>(&mut self, node: &'a SyntaxNode) -> Result<T> {
        T::eval(node, self.worker)
    }
}

// [attr] key: ty
macro_rules! get_args {
    (
        $args:expr,
        $key:ident: $ty:ty
    ) => {{
        let raw = $args.get(stringify!($key))?;
        $args.parse::<$ty>(raw)?
    }};
}

/// Evaluate a link to markdown-format string.
pub fn link(mut args: ArgGetter) -> Result<Value> {
    let dest = get_args!(args, dest: EcoString);
    let body = get_args!(args, body: &SyntaxNode);
    let body = args.worker.convert(body)?;

    Ok(Value::Content(eco_format!("[{body}]({dest})")))
}

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
