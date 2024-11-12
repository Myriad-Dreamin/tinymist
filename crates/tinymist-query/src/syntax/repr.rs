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

    pub fn write_decl(&mut self, d: &Decl) -> fmt::Result {
        write!(self.f, "{d:?}")
    }

    pub fn write_expr(&mut self, expr: &Expr) -> fmt::Result {
        match expr {
            Expr::Block(s) => self.write_seq(s),
            Expr::Array(a) => self.write_array(a),
            Expr::Dict(d) => self.write_dict(d),
            Expr::Args(a) => self.write_args(a),
            Expr::Pattern(p) => self.write_pattern(p),
            Expr::Element(e) => self.write_element(e),
            Expr::Unary(u) => self.write_unary(u),
            Expr::Binary(b) => self.write_binary(b),
            Expr::Apply(a) => self.write_apply(a),
            Expr::Func(func) => self.write_func(func),
            Expr::Let(l) => self.write_let(l),
            Expr::Show(s) => self.write_show(s),
            Expr::Set(s) => self.write_set(s),
            Expr::Ref(r) => self.write_ref(r),
            Expr::ContentRef(r) => self.write_content_ref(r),
            Expr::Select(s) => self.write_select(s),
            Expr::Import(i) => self.write_import(i),
            Expr::Include(i) => self.write_include(i),
            Expr::Contextual(c) => self.write_contextual(c),
            Expr::Conditional(c) => self.write_conditional(c),
            Expr::WhileLoop(w) => self.write_while_loop(w),
            Expr::ForLoop(f) => self.write_for_loop(f),
            Expr::Type(t) => self.write_type(t),
            Expr::Decl(d) => self.write_decl(d),
            Expr::Star => self.write_star(),
        }
    }

    fn write_indent(&mut self) -> fmt::Result {
        write!(self.f, "{:indent$}", "", indent = self.indent)
    }

    fn write_seq(&mut self, s: &Interned<Vec<Expr>>) -> fmt::Result {
        writeln!(self.f, "[")?;
        self.indent += 1;
        for expr in s.iter() {
            self.write_indent()?;
            self.write_expr(expr)?;
            self.f.write_str(",\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        write!(self.f, "]")
    }

    fn write_array(&mut self, a: &Interned<Vec<ArgExpr>>) -> fmt::Result {
        writeln!(self.f, "(")?;
        self.indent += 1;
        for arg in a.iter() {
            self.write_indent()?;
            self.write_arg(arg)?;
            self.f.write_str(",\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        write!(self.f, ")")
    }

    fn write_dict(&mut self, d: &Interned<Vec<ArgExpr>>) -> fmt::Result {
        writeln!(self.f, "(:")?;
        self.indent += 1;
        for arg in d.iter() {
            self.write_indent()?;
            self.write_arg(arg)?;
            self.f.write_str(",\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        write!(self.f, ")")
    }

    fn write_args(&mut self, a: &Interned<Vec<ArgExpr>>) -> fmt::Result {
        writeln!(self.f, "(")?;
        for arg in a.iter() {
            self.write_indent()?;
            self.write_arg(arg)?;
            self.f.write_str(",\n")?;
        }
        self.write_indent()?;
        write!(self.f, ")")
    }

    fn write_arg(&mut self, a: &ArgExpr) -> fmt::Result {
        match a {
            ArgExpr::Pos(e) => self.write_expr(e),
            ArgExpr::Named(n) => {
                let (k, v) = n.as_ref();
                write!(self.f, "{k:?}: ")?;
                self.write_expr(v)
            }
            ArgExpr::NamedRt(n) => {
                let n = n.as_ref();
                self.write_expr(&n.0)?;
                write!(self.f, ": ")?;
                self.write_expr(&n.1)
            }
            ArgExpr::Spread(e) => {
                write!(self.f, "..")?;
                self.write_expr(e)
            }
        }
    }

    pub fn write_pattern(&mut self, p: &Pattern) -> fmt::Result {
        match p {
            Pattern::Expr(e) => self.write_expr(e),
            Pattern::Simple(s) => self.write_decl(s),
            Pattern::Sig(p) => self.write_pattern_sig(p),
        }
    }

    fn write_pattern_sig(&mut self, p: &PatternSig) -> fmt::Result {
        self.f.write_str("pat(\n")?;
        self.indent += 1;
        for pos in &p.pos {
            self.write_indent()?;
            self.write_pattern(pos)?;
            self.f.write_str(",\n")?;
        }
        for (name, pat) in &p.named {
            self.write_indent()?;
            write!(self.f, "{name:?} = ")?;
            self.write_pattern(pat)?;
            self.f.write_str(",\n")?;
        }
        if let Some((k, rest)) = &p.spread_left {
            self.write_indent()?;
            write!(self.f, "..{k:?}: ")?;
            self.write_pattern(rest)?;
            self.f.write_str(",\n")?;
        }
        if let Some((k, rest)) = &p.spread_right {
            self.write_indent()?;
            write!(self.f, "..{k:?}: ")?;
            self.write_pattern(rest)?;
            self.f.write_str(",\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        self.f.write_str(")")
    }

    fn write_element(&mut self, e: &Interned<ElementExpr>) -> fmt::Result {
        self.f.write_str("elem(\n")?;
        self.indent += 1;
        for v in &e.content {
            self.write_indent()?;
            self.write_expr(v)?;
            self.f.write_str(",\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        self.f.write_str(")")
    }

    fn write_unary(&mut self, u: &Interned<UnExpr>) -> fmt::Result {
        write!(self.f, "un({:?})(", u.op)?;
        self.write_expr(&u.lhs)?;
        self.f.write_str(")")
    }

    fn write_binary(&mut self, b: &Interned<BinExpr>) -> fmt::Result {
        let [lhs, rhs] = b.operands();
        write!(self.f, "bin({:?})(", b.op)?;
        self.write_expr(lhs)?;
        self.f.write_str(", ")?;
        self.write_expr(rhs)?;
        self.f.write_str(")")
    }

    fn write_apply(&mut self, a: &Interned<ApplyExpr>) -> fmt::Result {
        write!(self.f, "apply(")?;
        self.write_expr(&a.callee)?;
        self.f.write_str(", ")?;
        self.write_expr(&a.args)?;
        write!(self.f, ")")
    }

    fn write_func(&mut self, func: &Interned<FuncExpr>) -> fmt::Result {
        write!(self.f, "func[{:?}](", func.decl)?;
        self.write_pattern_sig(&func.params)?;
        write!(self.f, " = ")?;
        self.write_expr(&func.body)?;
        write!(self.f, ")")
    }

    fn write_let(&mut self, l: &Interned<LetExpr>) -> fmt::Result {
        write!(self.f, "let(")?;
        self.write_pattern(&l.pattern)?;
        if let Some(body) = &l.body {
            write!(self.f, " = ")?;
            self.write_expr(body)?;
        }
        write!(self.f, ")")
    }

    fn write_show(&mut self, s: &Interned<ShowExpr>) -> fmt::Result {
        write!(self.f, "show(")?;
        if let Some(selector) = &s.selector {
            self.write_expr(selector)?;
            self.f.write_str(", ")?;
        }
        self.write_expr(&s.edit)?;
        write!(self.f, ")")
    }

    fn write_set(&mut self, s: &Interned<SetExpr>) -> fmt::Result {
        write!(self.f, "set(")?;
        self.write_expr(&s.target)?;
        self.f.write_str(", ")?;
        self.write_expr(&s.args)?;
        if let Some(cond) = &s.cond {
            self.f.write_str(", ")?;
            self.write_expr(cond)?;
        }
        write!(self.f, ")")
    }

    fn write_ref(&mut self, r: &Interned<RefExpr>) -> fmt::Result {
        write!(self.f, "ref({:?}", r.decl)?;
        if let Some(step) = &r.step {
            self.f.write_str(", step = ")?;
            self.write_expr(step)?;
        }
        if let Some(of) = &r.root {
            self.f.write_str(", root = ")?;
            self.write_expr(of)?;
        }
        if let Some(val) = &r.val {
            write!(self.f, ", val = {val:?}")?;
        }
        self.f.write_str(")")
    }

    fn write_content_ref(&mut self, r: &Interned<ContentRefExpr>) -> fmt::Result {
        write!(self.f, "content_ref({:?}", r.ident)?;
        if let Some(of) = &r.of {
            self.f.write_str(", ")?;
            self.write_decl(of)?;
        }
        if let Some(val) = &r.body {
            self.write_expr(val)?;
        }
        self.f.write_str(")")
    }

    fn write_select(&mut self, s: &Interned<SelectExpr>) -> fmt::Result {
        write!(self.f, "(")?;
        self.write_expr(&s.lhs)?;
        self.f.write_str(").")?;
        self.write_decl(&s.key)
    }

    fn write_import(&mut self, i: &Interned<ImportExpr>) -> fmt::Result {
        self.f.write_str("import(")?;
        self.write_decl(&i.decl)?;
        self.f.write_str(")")
    }

    fn write_include(&mut self, i: &Interned<IncludeExpr>) -> fmt::Result {
        self.f.write_str("include(")?;
        self.write_expr(&i.source)?;
        self.f.write_str(")")
    }

    fn write_contextual(&mut self, c: &Interned<Expr>) -> fmt::Result {
        self.f.write_str("contextual(")?;
        self.write_expr(c)?;
        self.f.write_str(")")
    }

    fn write_conditional(&mut self, c: &Interned<IfExpr>) -> fmt::Result {
        self.f.write_str("if(")?;
        self.write_expr(&c.cond)?;
        self.f.write_str(", then = ")?;
        self.write_expr(&c.then)?;
        self.f.write_str(", else = ")?;
        self.write_expr(&c.else_)?;
        self.f.write_str(")")
    }

    fn write_while_loop(&mut self, w: &Interned<WhileExpr>) -> fmt::Result {
        self.f.write_str("while(")?;
        self.write_expr(&w.cond)?;
        self.f.write_str(", ")?;
        self.write_expr(&w.body)?;
        self.f.write_str(")")
    }

    fn write_for_loop(&mut self, f: &Interned<ForExpr>) -> fmt::Result {
        self.f.write_str("for(")?;
        self.write_pattern(&f.pattern)?;
        self.f.write_str(", ")?;
        self.write_expr(&f.iter)?;
        self.f.write_str(", ")?;
        self.write_expr(&f.body)?;
        self.f.write_str(")")
    }

    fn write_type(&mut self, t: &Ty) -> fmt::Result {
        let formatted = t.describe();
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

    pub fn write_decl(&mut self, d: &Decl) -> fmt::Result {
        use DefKind::*;
        let shorter = matches!(d.kind(), Function | Variable | Module);
        if shorter && !d.name().is_empty() {
            return write!(self.f, "{}", d.name());
        }

        write!(self.f, "{d:?}")
    }

    pub fn write_expr(&mut self, expr: &Expr) -> fmt::Result {
        match expr {
            Expr::Block(..) => self.f.write_str("Expr(..)"),
            Expr::Array(a) => self.write_array(a),
            Expr::Dict(d) => self.write_dict(d),
            Expr::Args(a) => self.write_args(a),
            Expr::Pattern(p) => self.write_pattern(p),
            Expr::Element(e) => self.write_element(e),
            Expr::Unary(u) => self.write_unary(u),
            Expr::Binary(b) => self.write_binary(b),
            Expr::Apply(a) => self.write_apply(a),
            Expr::Func(func) => self.write_func(func),
            Expr::Ref(r) => self.write_ref(r),
            Expr::ContentRef(r) => self.write_content_ref(r),
            Expr::Select(s) => self.write_select(s),
            Expr::Import(i) => self.write_import(i),
            Expr::Include(i) => self.write_include(i),
            Expr::Contextual(..) => self.f.write_str("content"),
            Expr::Let(..) | Expr::Show(..) | Expr::Set(..) => self.f.write_str("Expr(..)"),
            Expr::Conditional(..) | Expr::WhileLoop(..) | Expr::ForLoop(..) => {
                self.f.write_str("Expr(..)")
            }
            Expr::Type(t) => self.write_type(t),
            Expr::Decl(d) => self.write_decl(d),
            Expr::Star => self.f.write_str("*"),
        }
    }

    fn write_indent(&mut self) -> fmt::Result {
        write!(self.f, "{:indent$}", "", indent = self.indent)
    }

    fn write_array(&mut self, a: &Interned<Vec<ArgExpr>>) -> fmt::Result {
        if a.len() <= 1 {
            self.f.write_char('(')?;
            if let Some(arg) = a.first() {
                self.write_arg(arg)?;
                self.f.write_str(",")?
            }
            return self.f.write_str(")");
        }

        writeln!(self.f, "(")?;
        self.indent += 1;
        for arg in a.iter() {
            self.write_indent()?;
            self.write_arg(arg)?;
            self.f.write_str(",\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        write!(self.f, ")")
    }

    fn write_dict(&mut self, d: &Interned<Vec<ArgExpr>>) -> fmt::Result {
        if d.len() <= 1 {
            self.f.write_char('(')?;
            if let Some(arg) = d.first() {
                self.write_arg(arg)?;
            } else {
                self.f.write_str(":")?
            }
            return self.f.write_str(")");
        }

        writeln!(self.f, "(:")?;
        self.indent += 1;
        for arg in d.iter() {
            self.write_indent()?;
            self.write_arg(arg)?;
            self.f.write_str(",\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        write!(self.f, ")")
    }

    fn write_args(&mut self, a: &Interned<Vec<ArgExpr>>) -> fmt::Result {
        writeln!(self.f, "(")?;
        for arg in a.iter() {
            self.write_indent()?;
            self.write_arg(arg)?;
            self.f.write_str(",\n")?;
        }
        self.write_indent()?;
        write!(self.f, ")")
    }

    fn write_arg(&mut self, a: &ArgExpr) -> fmt::Result {
        match a {
            ArgExpr::Pos(e) => self.write_expr(e),
            ArgExpr::Named(n) => {
                let (k, v) = n.as_ref();
                self.write_decl(k)?;
                write!(self.f, ": ")?;
                self.write_expr(v)
            }
            ArgExpr::NamedRt(n) => {
                let n = n.as_ref();
                self.write_expr(&n.0)?;
                write!(self.f, ": ")?;
                self.write_expr(&n.1)
            }
            ArgExpr::Spread(e) => {
                write!(self.f, "..")?;
                self.write_expr(e)
            }
        }
    }

    pub fn write_pattern(&mut self, p: &Pattern) -> fmt::Result {
        match p {
            Pattern::Expr(e) => self.write_expr(e),
            Pattern::Simple(s) => self.write_decl(s),
            Pattern::Sig(p) => self.write_pattern_sig(p),
        }
    }

    fn write_pattern_sig(&mut self, p: &PatternSig) -> fmt::Result {
        self.f.write_str("pat(\n")?;
        self.indent += 1;
        for pos in &p.pos {
            self.write_indent()?;
            self.write_pattern(pos)?;
            self.f.write_str(",\n")?;
        }
        for (name, pat) in &p.named {
            self.write_indent()?;
            write!(self.f, "{name:?} = ")?;
            self.write_pattern(pat)?;
            self.f.write_str(",\n")?;
        }
        if let Some((k, rest)) = &p.spread_left {
            self.write_indent()?;
            write!(self.f, "..{k:?}: ")?;
            self.write_pattern(rest)?;
            self.f.write_str(",\n")?;
        }
        if let Some((k, rest)) = &p.spread_right {
            self.write_indent()?;
            write!(self.f, "..{k:?}: ")?;
            self.write_pattern(rest)?;
            self.f.write_str(",\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        self.f.write_str(")")
    }

    fn write_element(&mut self, e: &Interned<ElementExpr>) -> fmt::Result {
        write!(self.f, "{:?}", e.elem.name())
    }

    fn write_unary(&mut self, u: &Interned<UnExpr>) -> fmt::Result {
        use UnaryOp::*;
        match u.op {
            Pos => {
                self.f.write_str("+")?;
                self.write_expr(&u.lhs)
            }
            Neg => {
                self.f.write_str("-")?;
                self.write_expr(&u.lhs)
            }
            Not => {
                self.f.write_str("not ")?;
                self.write_expr(&u.lhs)
            }
            Return => {
                self.f.write_str("return ")?;
                self.write_expr(&u.lhs)
            }
            Context => {
                self.f.write_str("context ")?;
                self.write_expr(&u.lhs)
            }
            Spread => {
                self.f.write_str("..")?;
                self.write_expr(&u.lhs)
            }
            NotElementOf => {
                self.f.write_str("not elementOf(")?;
                self.write_expr(&u.lhs)?;
                self.f.write_str(")")
            }
            ElementOf => {
                self.f.write_str("elementOf(")?;
                self.write_expr(&u.lhs)?;
                self.f.write_str(")")
            }
            TypeOf => {
                self.f.write_str("typeOf(")?;
                self.write_expr(&u.lhs)?;
                self.f.write_str(")")
            }
        }
    }

    fn write_binary(&mut self, b: &Interned<BinExpr>) -> fmt::Result {
        let [lhs, rhs] = b.operands();
        self.write_expr(lhs)?;
        write!(self.f, " {} ", b.op.as_str())?;
        self.write_expr(rhs)
    }

    fn write_apply(&mut self, a: &Interned<ApplyExpr>) -> fmt::Result {
        self.write_expr(&a.callee)?;
        write!(self.f, "(")?;
        self.write_expr(&a.args)?;
        write!(self.f, ")")
    }

    fn write_func(&mut self, func: &Interned<FuncExpr>) -> fmt::Result {
        self.write_decl(&func.decl)
    }

    fn write_ref(&mut self, r: &Interned<RefExpr>) -> fmt::Result {
        if let Some(r) = &r.root {
            return self.write_expr(r);
        }
        if let Some(r) = &r.val {
            return self.write_type(r);
        }

        write!(self.f, "undefined({:?})", r.decl)
    }

    fn write_content_ref(&mut self, r: &Interned<ContentRefExpr>) -> fmt::Result {
        write!(self.f, "@{:?}", r.ident)
    }

    fn write_select(&mut self, s: &Interned<SelectExpr>) -> fmt::Result {
        write!(self.f, "")?;
        self.write_expr(&s.lhs)?;
        self.f.write_str(".")?;
        self.write_decl(&s.key)
    }

    fn write_import(&mut self, i: &Interned<ImportExpr>) -> fmt::Result {
        self.f.write_str("import(")?;
        self.write_decl(&i.decl)?;
        self.f.write_str(")")
    }

    fn write_include(&mut self, i: &Interned<IncludeExpr>) -> fmt::Result {
        self.f.write_str("include(")?;
        self.write_expr(&i.source)?;
        self.f.write_str(")")
    }

    fn write_type(&mut self, t: &Ty) -> fmt::Result {
        let formatted = t.describe();
        let formatted = formatted.as_deref().unwrap_or("any");
        self.f.write_str(formatted)
    }
}
