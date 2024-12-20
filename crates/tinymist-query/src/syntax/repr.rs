use core::fmt;

use super::def::*;
use crate::ty::{Interned, Ty};

pub(in crate::syntax) struct ExprPrinter<'a, T: fmt::Write> {
    f: &'a mut T,
    indent: usize,
}

impl<'a, T: fmt::Write> ExprPrinter<'a, T> {
    pub fn new(f: &'a mut T) -> Self {
        Self { f, indent: 0 }
    }

    pub fn write_decl(&mut self, decl: &Decl) -> fmt::Result {
        write!(self.f, "{decl:?}")
    }

    pub fn write_expr(&mut self, expr: &Expr) -> fmt::Result {
        match expr {
            Expr::Deferred(..) => self.f.write_str("Deferred(..)"),
            Expr::Block(exprs) => self.write_seq(exprs),
            Expr::Array(elems) => self.write_array(elems),
            Expr::Dict(elems) => self.write_dict(elems),
            Expr::Args(args) => self.write_args(args),
            Expr::Pattern(pat) => self.write_pattern(pat),
            Expr::Element(elem) => self.write_element(elem),
            Expr::Unary(unary) => self.write_unary(unary),
            Expr::Binary(binary) => self.write_binary(binary),
            Expr::Apply(apply) => self.write_apply(apply),
            Expr::Func(func) => self.write_func(func),
            Expr::Let(let_expr) => self.write_let(let_expr),
            Expr::Show(show) => self.write_show(show),
            Expr::Set(set) => self.write_set(set),
            Expr::Ref(reference) => self.write_ref(reference),
            Expr::ContentRef(content_ref) => self.write_content_ref(content_ref),
            Expr::Select(sel) => self.write_select(sel),
            Expr::Import(import) => self.write_import(import),
            Expr::Include(include) => self.write_include(include),
            Expr::Contextual(contextual) => self.write_contextual(contextual),
            Expr::Conditional(if_expr) => self.write_conditional(if_expr),
            Expr::WhileLoop(while_expr) => self.write_while_loop(while_expr),
            Expr::ForLoop(for_expr) => self.write_for_loop(for_expr),
            Expr::Type(ty) => self.write_type(ty),
            Expr::Decl(decl) => self.write_decl(decl),
            Expr::Star => self.write_star(),
        }
    }

    fn write_indent(&mut self) -> fmt::Result {
        write!(self.f, "{:indent$}", "", indent = self.indent)
    }

    fn write_seq(&mut self, exprs: &Interned<Vec<Expr>>) -> fmt::Result {
        writeln!(self.f, "[")?;
        self.indent += 1;
        for expr in exprs.iter() {
            self.write_indent()?;
            self.write_expr(expr)?;
            self.f.write_str(",\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        write!(self.f, "]")
    }

    fn write_array(&mut self, elems: &Interned<Vec<ArgExpr>>) -> fmt::Result {
        writeln!(self.f, "(")?;
        self.indent += 1;
        for arg in elems.iter() {
            self.write_indent()?;
            self.write_arg(arg)?;
            self.f.write_str(",\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        write!(self.f, ")")
    }

    fn write_dict(&mut self, elems: &Interned<Vec<ArgExpr>>) -> fmt::Result {
        writeln!(self.f, "(:")?;
        self.indent += 1;
        for arg in elems.iter() {
            self.write_indent()?;
            self.write_arg(arg)?;
            self.f.write_str(",\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        write!(self.f, ")")
    }

    fn write_args(&mut self, args: &Interned<Vec<ArgExpr>>) -> fmt::Result {
        writeln!(self.f, "(")?;
        for arg in args.iter() {
            self.write_indent()?;
            self.write_arg(arg)?;
            self.f.write_str(",\n")?;
        }
        self.write_indent()?;
        write!(self.f, ")")
    }

    fn write_arg(&mut self, arg: &ArgExpr) -> fmt::Result {
        match arg {
            ArgExpr::Pos(pos) => self.write_expr(pos),
            ArgExpr::Named(named) => {
                let (name, val) = named.as_ref();
                write!(self.f, "{name:?}: ")?;
                self.write_expr(val)
            }
            ArgExpr::NamedRt(named) => {
                let (key, val) = named.as_ref();
                self.write_expr(key)?;
                write!(self.f, ": ")?;
                self.write_expr(val)
            }
            ArgExpr::Spread(spread) => {
                write!(self.f, "..")?;
                self.write_expr(spread)
            }
        }
    }

    pub fn write_pattern(&mut self, pat: &Pattern) -> fmt::Result {
        match pat {
            Pattern::Expr(expr) => self.write_expr(expr),
            Pattern::Simple(decl) => self.write_decl(decl),
            Pattern::Sig(sig) => self.write_pattern_sig(sig),
        }
    }

    fn write_pattern_sig(&mut self, sig: &PatternSig) -> fmt::Result {
        self.f.write_str("pat(\n")?;
        self.indent += 1;
        for pos in &sig.pos {
            self.write_indent()?;
            self.write_pattern(pos)?;
            self.f.write_str(",\n")?;
        }
        for (name, named) in &sig.named {
            self.write_indent()?;
            write!(self.f, "{name:?} = ")?;
            self.write_pattern(named)?;
            self.f.write_str(",\n")?;
        }
        if let Some((name, spread_left)) = &sig.spread_left {
            self.write_indent()?;
            write!(self.f, "..{name:?}: ")?;
            self.write_pattern(spread_left)?;
            self.f.write_str(",\n")?;
        }
        if let Some((name, spread_right)) = &sig.spread_right {
            self.write_indent()?;
            write!(self.f, "..{name:?}: ")?;
            self.write_pattern(spread_right)?;
            self.f.write_str(",\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        self.f.write_str(")")
    }

    fn write_element(&mut self, elem: &Interned<ElementExpr>) -> fmt::Result {
        self.f.write_str("elem(\n")?;
        self.indent += 1;
        for v in &elem.content {
            self.write_indent()?;
            self.write_expr(v)?;
            self.f.write_str(",\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        self.f.write_str(")")
    }

    fn write_unary(&mut self, unary: &Interned<UnExpr>) -> fmt::Result {
        write!(self.f, "un({:?})(", unary.op)?;
        self.write_expr(&unary.lhs)?;
        self.f.write_str(")")
    }

    fn write_binary(&mut self, binary: &Interned<BinExpr>) -> fmt::Result {
        let [lhs, rhs] = binary.operands();
        write!(self.f, "bin({:?})(", binary.op)?;
        self.write_expr(lhs)?;
        self.f.write_str(", ")?;
        self.write_expr(rhs)?;
        self.f.write_str(")")
    }

    fn write_apply(&mut self, apply: &Interned<ApplyExpr>) -> fmt::Result {
        write!(self.f, "apply(")?;
        self.write_expr(&apply.callee)?;
        self.f.write_str(", ")?;
        self.write_expr(&apply.args)?;
        write!(self.f, ")")
    }

    fn write_func(&mut self, func: &Interned<FuncExpr>) -> fmt::Result {
        write!(self.f, "func[{:?}](", func.decl)?;
        self.write_pattern_sig(&func.params)?;
        write!(self.f, " = ")?;
        self.write_expr(&func.body)?;
        write!(self.f, ")")
    }

    fn write_let(&mut self, let_expr: &Interned<LetExpr>) -> fmt::Result {
        write!(self.f, "let(")?;
        self.write_pattern(&let_expr.pattern)?;
        if let Some(body) = &let_expr.body {
            write!(self.f, " = ")?;
            self.write_expr(body)?;
        }
        write!(self.f, ")")
    }

    fn write_show(&mut self, show: &Interned<ShowExpr>) -> fmt::Result {
        write!(self.f, "show(")?;
        if let Some(selector) = &show.selector {
            self.write_expr(selector)?;
            self.f.write_str(", ")?;
        }
        self.write_expr(&show.edit)?;
        write!(self.f, ")")
    }

    fn write_set(&mut self, set: &Interned<SetExpr>) -> fmt::Result {
        write!(self.f, "set(")?;
        self.write_expr(&set.target)?;
        self.f.write_str(", ")?;
        self.write_expr(&set.args)?;
        if let Some(cond) = &set.cond {
            self.f.write_str(", ")?;
            self.write_expr(cond)?;
        }
        write!(self.f, ")")
    }

    fn write_ref(&mut self, reference: &Interned<RefExpr>) -> fmt::Result {
        write!(self.f, "ref({:?}", reference.decl)?;
        if let Some(step) = &reference.step {
            self.f.write_str(", step = ")?;
            self.write_expr(step)?;
        }
        if let Some(of) = &reference.root {
            self.f.write_str(", root = ")?;
            self.write_expr(of)?;
        }
        if let Some(val) = &reference.term {
            write!(self.f, ", val = {val:?}")?;
        }
        self.f.write_str(")")
    }

    fn write_content_ref(&mut self, content_ref: &Interned<ContentRefExpr>) -> fmt::Result {
        write!(self.f, "content_ref({:?}", content_ref.ident)?;
        if let Some(of) = &content_ref.of {
            self.f.write_str(", ")?;
            self.write_decl(of)?;
        }
        if let Some(val) = &content_ref.body {
            self.write_expr(val)?;
        }
        self.f.write_str(")")
    }

    fn write_select(&mut self, sel: &Interned<SelectExpr>) -> fmt::Result {
        write!(self.f, "(")?;
        self.write_expr(&sel.lhs)?;
        self.f.write_str(").")?;
        self.write_decl(&sel.key)
    }

    fn write_import(&mut self, import: &Interned<ImportExpr>) -> fmt::Result {
        self.f.write_str("import(")?;
        self.write_decl(&import.decl.decl)?;
        self.f.write_str(")")
    }

    fn write_include(&mut self, include: &Interned<IncludeExpr>) -> fmt::Result {
        self.f.write_str("include(")?;
        self.write_expr(&include.source)?;
        self.f.write_str(")")
    }

    fn write_contextual(&mut self, contextual: &Interned<Expr>) -> fmt::Result {
        self.f.write_str("contextual(")?;
        self.write_expr(contextual)?;
        self.f.write_str(")")
    }

    fn write_conditional(&mut self, if_expr: &Interned<IfExpr>) -> fmt::Result {
        self.f.write_str("if(")?;
        self.write_expr(&if_expr.cond)?;
        self.f.write_str(", then = ")?;
        self.write_expr(&if_expr.then)?;
        self.f.write_str(", else = ")?;
        self.write_expr(&if_expr.else_)?;
        self.f.write_str(")")
    }

    fn write_while_loop(&mut self, while_expr: &Interned<WhileExpr>) -> fmt::Result {
        self.f.write_str("while(")?;
        self.write_expr(&while_expr.cond)?;
        self.f.write_str(", ")?;
        self.write_expr(&while_expr.body)?;
        self.f.write_str(")")
    }

    fn write_for_loop(&mut self, for_expr: &Interned<ForExpr>) -> fmt::Result {
        self.f.write_str("for(")?;
        self.write_pattern(&for_expr.pattern)?;
        self.f.write_str(", ")?;
        self.write_expr(&for_expr.iter)?;
        self.f.write_str(", ")?;
        self.write_expr(&for_expr.body)?;
        self.f.write_str(")")
    }

    fn write_type(&mut self, ty: &Ty) -> fmt::Result {
        let formatted = ty.describe();
        let formatted = formatted.as_deref().unwrap_or("any");
        self.f.write_str(formatted)
    }

    fn write_star(&mut self) -> fmt::Result {
        self.f.write_str("*")
    }
}

pub(in crate::syntax) struct ExprDescriber<'a, T: fmt::Write> {
    f: &'a mut T,
    indent: usize,
}

impl<'a, T: fmt::Write> ExprDescriber<'a, T> {
    pub fn new(f: &'a mut T) -> Self {
        Self { f, indent: 0 }
    }

    pub fn write_decl(&mut self, decl: &Decl) -> fmt::Result {
        use DefKind::*;
        let shorter = matches!(decl.kind(), Function | Variable | Module);
        if shorter && !decl.name().is_empty() {
            return write!(self.f, "{}", decl.name());
        }

        write!(self.f, "{decl:?}")
    }

    pub fn write_expr(&mut self, expr: &Expr) -> fmt::Result {
        match expr {
            Expr::Deferred(..) => self.f.write_str("Deferred(..)"),
            Expr::Block(..) => self.f.write_str("Expr(..)"),
            Expr::Array(elems) => self.write_array(elems),
            Expr::Dict(elems) => self.write_dict(elems),
            Expr::Args(args) => self.write_args(args),
            Expr::Pattern(pat) => self.write_pattern(pat),
            Expr::Element(elem) => self.write_element(elem),
            Expr::Unary(unary) => self.write_unary(unary),
            Expr::Binary(binary) => self.write_binary(binary),
            Expr::Apply(apply) => self.write_apply(apply),
            Expr::Func(func) => self.write_func(func),
            Expr::Ref(ref_expr) => self.write_ref(ref_expr),
            Expr::ContentRef(content_ref) => self.write_content_ref(content_ref),
            Expr::Select(sel) => self.write_select(sel),
            Expr::Import(import) => self.write_import(import),
            Expr::Include(include) => self.write_include(include),
            Expr::Contextual(..) => self.f.write_str("content"),
            Expr::Let(..) | Expr::Show(..) | Expr::Set(..) => self.f.write_str("Expr(..)"),
            Expr::Conditional(..) | Expr::WhileLoop(..) | Expr::ForLoop(..) => {
                self.f.write_str("Expr(..)")
            }
            Expr::Type(ty) => self.write_type(ty),
            Expr::Decl(decl) => self.write_decl(decl),
            Expr::Star => self.f.write_str("*"),
        }
    }

    fn write_indent(&mut self) -> fmt::Result {
        write!(self.f, "{:indent$}", "", indent = self.indent)
    }

    fn write_array(&mut self, elems: &Interned<Vec<ArgExpr>>) -> fmt::Result {
        if elems.len() <= 1 {
            self.f.write_char('(')?;
            if let Some(arg) = elems.first() {
                self.write_arg(arg)?;
                self.f.write_str(",")?
            }
            return self.f.write_str(")");
        }

        writeln!(self.f, "(")?;
        self.indent += 1;
        for arg in elems.iter() {
            self.write_indent()?;
            self.write_arg(arg)?;
            self.f.write_str(",\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        write!(self.f, ")")
    }

    fn write_dict(&mut self, elems: &Interned<Vec<ArgExpr>>) -> fmt::Result {
        if elems.len() <= 1 {
            self.f.write_char('(')?;
            if let Some(arg) = elems.first() {
                self.write_arg(arg)?;
            } else {
                self.f.write_str(":")?
            }
            return self.f.write_str(")");
        }

        writeln!(self.f, "(:")?;
        self.indent += 1;
        for arg in elems.iter() {
            self.write_indent()?;
            self.write_arg(arg)?;
            self.f.write_str(",\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        write!(self.f, ")")
    }

    fn write_args(&mut self, args: &Interned<Vec<ArgExpr>>) -> fmt::Result {
        writeln!(self.f, "(")?;
        for arg in args.iter() {
            self.write_indent()?;
            self.write_arg(arg)?;
            self.f.write_str(",\n")?;
        }
        self.write_indent()?;
        write!(self.f, ")")
    }

    fn write_arg(&mut self, arg: &ArgExpr) -> fmt::Result {
        match arg {
            ArgExpr::Pos(pos) => self.write_expr(pos),
            ArgExpr::Named(named) => {
                let (k, v) = named.as_ref();
                self.write_decl(k)?;
                write!(self.f, ": ")?;
                self.write_expr(v)
            }
            ArgExpr::NamedRt(named) => {
                let n = named.as_ref();
                self.write_expr(&n.0)?;
                write!(self.f, ": ")?;
                self.write_expr(&n.1)
            }
            ArgExpr::Spread(spread) => {
                write!(self.f, "..")?;
                self.write_expr(spread)
            }
        }
    }

    pub fn write_pattern(&mut self, pat: &Pattern) -> fmt::Result {
        match pat {
            Pattern::Expr(expr) => self.write_expr(expr),
            Pattern::Simple(decl) => self.write_decl(decl),
            Pattern::Sig(sig) => self.write_pattern_sig(sig),
        }
    }

    fn write_pattern_sig(&mut self, sig: &PatternSig) -> fmt::Result {
        self.f.write_str("pat(\n")?;
        self.indent += 1;
        for pos in &sig.pos {
            self.write_indent()?;
            self.write_pattern(pos)?;
            self.f.write_str(",\n")?;
        }
        for (name, pat) in &sig.named {
            self.write_indent()?;
            write!(self.f, "{name:?} = ")?;
            self.write_pattern(pat)?;
            self.f.write_str(",\n")?;
        }
        if let Some((k, rest)) = &sig.spread_left {
            self.write_indent()?;
            write!(self.f, "..{k:?}: ")?;
            self.write_pattern(rest)?;
            self.f.write_str(",\n")?;
        }
        if let Some((k, rest)) = &sig.spread_right {
            self.write_indent()?;
            write!(self.f, "..{k:?}: ")?;
            self.write_pattern(rest)?;
            self.f.write_str(",\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        self.f.write_str(")")
    }

    fn write_element(&mut self, elem: &Interned<ElementExpr>) -> fmt::Result {
        write!(self.f, "{:?}", elem.elem.name())
    }

    fn write_unary(&mut self, unary: &Interned<UnExpr>) -> fmt::Result {
        use UnaryOp::*;
        match unary.op {
            Pos => {
                self.f.write_str("+")?;
                self.write_expr(&unary.lhs)
            }
            Neg => {
                self.f.write_str("-")?;
                self.write_expr(&unary.lhs)
            }
            Not => {
                self.f.write_str("not ")?;
                self.write_expr(&unary.lhs)
            }
            Return => {
                self.f.write_str("return ")?;
                self.write_expr(&unary.lhs)
            }
            Context => {
                self.f.write_str("context ")?;
                self.write_expr(&unary.lhs)
            }
            Spread => {
                self.f.write_str("..")?;
                self.write_expr(&unary.lhs)
            }
            NotElementOf => {
                self.f.write_str("not elementOf(")?;
                self.write_expr(&unary.lhs)?;
                self.f.write_str(")")
            }
            ElementOf => {
                self.f.write_str("elementOf(")?;
                self.write_expr(&unary.lhs)?;
                self.f.write_str(")")
            }
            TypeOf => {
                self.f.write_str("typeOf(")?;
                self.write_expr(&unary.lhs)?;
                self.f.write_str(")")
            }
        }
    }

    fn write_binary(&mut self, binary: &Interned<BinExpr>) -> fmt::Result {
        let [lhs, rhs] = binary.operands();
        self.write_expr(lhs)?;
        write!(self.f, " {} ", binary.op.as_str())?;
        self.write_expr(rhs)
    }

    fn write_apply(&mut self, apply: &Interned<ApplyExpr>) -> fmt::Result {
        self.write_expr(&apply.callee)?;
        write!(self.f, "(")?;
        self.write_expr(&apply.args)?;
        write!(self.f, ")")
    }

    fn write_func(&mut self, func: &Interned<FuncExpr>) -> fmt::Result {
        self.write_decl(&func.decl)
    }

    fn write_ref(&mut self, resolved: &Interned<RefExpr>) -> fmt::Result {
        if let Some(root) = &resolved.root {
            return self.write_expr(root);
        }
        if let Some(term) = &resolved.term {
            return self.write_type(term);
        }

        write!(self.f, "undefined({:?})", resolved.decl)
    }

    fn write_content_ref(&mut self, content_ref: &Interned<ContentRefExpr>) -> fmt::Result {
        write!(self.f, "@{:?}", content_ref.ident)
    }

    fn write_select(&mut self, sel: &Interned<SelectExpr>) -> fmt::Result {
        write!(self.f, "")?;
        self.write_expr(&sel.lhs)?;
        self.f.write_str(".")?;
        self.write_decl(&sel.key)
    }

    fn write_import(&mut self, import: &Interned<ImportExpr>) -> fmt::Result {
        self.f.write_str("import(")?;
        self.write_decl(&import.decl.decl)?;
        self.f.write_str(")")
    }

    fn write_include(&mut self, include: &Interned<IncludeExpr>) -> fmt::Result {
        self.f.write_str("include(")?;
        self.write_expr(&include.source)?;
        self.f.write_str(")")
    }

    fn write_type(&mut self, ty: &Ty) -> fmt::Result {
        let formatted = ty.describe();
        let formatted = formatted.as_deref().unwrap_or("any");
        self.f.write_str(formatted)
    }
}
