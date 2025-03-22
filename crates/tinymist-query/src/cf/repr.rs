use core::fmt;

use super::{def::*, CfInfo};

pub struct CfRepr<'a> {
    pub cf: &'a CfInfo,
    pub spaned: bool,
}

impl fmt::Display for CfRepr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let root = self.cf.root;
        writeln!(f, "document reg{root} {{")?;
        let mut printer = CfPrinter {
            cf: self.cf,
            spaned: self.spaned,
            indent: 1,
        };
        printer.region(root, f)?;
        writeln!(f, "}}")
    }
}

pub struct CfPrinter<'a> {
    cf: &'a CfInfo,
    spaned: bool,
    indent: usize,
}

impl CfPrinter<'_> {
    fn indent(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for _ in 0..self.indent {
            write!(f, "  ")?;
        }
        Ok(())
    }
    fn region(&mut self, id: RegionId, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let region = self.cf.region.get(id);
        self.indent(f)?;
        writeln!(f, "reg{id} {{")?;
        self.indent += 1;
        for (_idx, bb) in &region.basic_blocks {
            self.basic_block(region, bb, f)?;
        }
        self.indent -= 1;
        self.indent(f)?;
        writeln!(f, "}}")
    }

    fn basic_block(
        &mut self,
        reg: &Region,
        id: &BasicBlock,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        self.indent(f)?;
        writeln!(f, "^bb{}: {{", id.id)?;
        self.indent += 1;
        for idx in &id.nodes {
            self.node(*idx, reg.nodes.get(*idx), f)?;
            writeln!(f)?;
        }
        self.indent -= 1;
        self.indent(f)?;
        writeln!(f, "}}")
    }

    fn node(&mut self, id: NodeId, node: &CfNode, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.indent(f)?;
        write!(f, "%{id}")?;
        if self.spaned {
            write!(f, "loc({:?})", node.span)?;
        }
        match &node.instr {
            CfInstr::Let(cf_let) => {
                write!(f, ": {}", cf_let.ty.repr_any())?;
                write!(f, "let {}", cf_let.pattern.repr())?;
                if let Some(init) = cf_let.init {
                    write!(f, " = %{init}")?;
                }
            }
            // todo: assign is not good
            CfInstr::Assign(cf_assign) => {
                write!(f, " = %{}", cf_assign.lhs)?;
                write!(f, " = %{}", cf_assign.rhs)?;
            }
            CfInstr::Bin(cf_bin) => {
                write!(f, " = %{}", cf_bin.lhs)?;
                write!(f, " {:?}", cf_bin.op)?;
                write!(f, " %{}", cf_bin.rhs)?;
            }
            CfInstr::Un(cf_un) => {
                write!(f, " = {:?}", cf_un.op)?;
                write!(f, " %{}", cf_un.lhs)?;
            }
            CfInstr::Select(cf_select) => {
                write!(f, ": {}", cf_select.ty.repr_any())?;
                write!(f, " = select %{}", cf_select.lhs)?;
                write!(f, " {}", cf_select.key.repr())?;
            }
            CfInstr::Apply(cf_call) => {
                write!(f, ": {}", cf_call.ty.repr_any())?;
                write!(f, " = apply %{}", cf_call.func)?;
                write!(f, " %{}", cf_call.args)?;
            }
            CfInstr::Func(cf_func) => {
                write!(f, ": {}", cf_func.ty.repr_any())?;
                write!(f, " = func region{}", cf_func.body)?;
            }
            CfInstr::Array(cf_args) => {
                write!(f, " = ")?;
                self.args(cf_args, "array", f)?;
            }
            CfInstr::Dict(cf_args) => {
                write!(f, " = ")?;
                self.args(cf_args, "dict", f)?;
            }
            CfInstr::Args(cf_args) => {
                write!(f, " = ")?;
                self.args(cf_args, "args", f)?;
            }
            CfInstr::If(cf_if) => {
                write!(f, ": {}", cf_if.ty.repr_any())?;
                write!(f, " = if %{}", cf_if.cond)?;
                write!(f, " then %{}", cf_if.then)?;
                write!(f, " else %{}", cf_if.else_)?;
            }
            CfInstr::Loop(cf_loop) => {
                write!(f, ": {}", cf_loop.ty.repr_any())?;
                write!(f, " = loop cond %{}", cf_loop.cond)?;
                write!(f, " body ^bb{}", cf_loop.body)?;
                if let Some(cont) = cf_loop.cont {
                    write!(f, " cont ^bb{cont}")?;
                }
            }
            CfInstr::Block(cf_block) => {
                write!(f, ": {}", cf_block.ty.repr_any())?;
                write!(f, " = ^bb{}", cf_block.body)?;
            }
            CfInstr::Element(cf_element) => {
                // pub elem: Element,
                // pub body: BasicBlockId,

                write!(
                    f,
                    " = elem({}, ^bb{})",
                    cf_element.elem.name(),
                    cf_element.body
                )?;
            }
            CfInstr::Contextual(index_vec_idx) => {
                write!(f, " = context ^bb{index_vec_idx}")?;
            }
            CfInstr::Show(cf_show) => {
                write!(f, " = show ^bb{}", cf_show.cont)?;
                if let Some(selector) = cf_show.selector {
                    write!(f, " select %{selector}")?;
                }
                write!(f, " using %{}", cf_show.edit)?;
            }
            CfInstr::Set(cf_set) => {
                write!(f, " = set ^bb{}", cf_set.cont)?;
                write!(f, " %{}", cf_set.target)?;
                write!(f, " with %{}", cf_set.args)?;
            }
            CfInstr::Break(cont) => {
                if let Some(cont) = cont {
                    write!(f, " = break ^bb{cont}")?;
                } else {
                    write!(f, " = break undef")?;
                }
            }
            CfInstr::Continue(cont) => {
                if let Some(cont) = cont {
                    write!(f, " = continue ^bb{cont}")?;
                } else {
                    write!(f, " = continue undef")?;
                }
            }
            CfInstr::Return(value) => {
                if let Some(value) = value {
                    write!(f, " = return %{value}")?;
                } else {
                    write!(f, " = return")?;
                }
            }
            CfInstr::Branch(index_vec_idx) => {
                write!(f, " = branch ^bb{index_vec_idx}")?;
            }
            CfInstr::Meta(expr) => {
                write!(f, " = meta {}", expr.repr())?;
            }
            CfInstr::Undef(interned) => {
                write!(f, " = undef {}", interned.repr())?;
            }
            CfInstr::Include(index_vec_idx) => {
                write!(f, " = include %{index_vec_idx}")?;
            }
            CfInstr::Iter(index_vec_idx) => {
                write!(f, " = iter %{index_vec_idx}")?;
            }
            CfInstr::Ins(ty) => {
                write!(f, " = ins {}", ty.repr_any())?;
            }
        }

        Ok(())
    }

    fn args(&mut self, id: &CfArgs, kind: &'static str, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{kind}")?;
        write!(f, "(")?;
        let mut first = false;
        for arg in &id.args {
            if !first {
                write!(f, ", ")?;
            } else {
                first = true;
            }
            self.arg(arg, f)?;
        }
        write!(f, ")")?;
        Ok(())
    }

    fn arg(&self, id: &CfArg, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match id {
            CfArg::Pos(node_id) => write!(f, "%{node_id}"),
            CfArg::Named(decl, node_id) => write!(f, "{} -> %{node_id}", decl.repr()),
            CfArg::NamedRt(node_id, rt) => write!(f, "%{rt} -> %{node_id}"),
            CfArg::Spread(node_id) => write!(f, "..%{node_id}"),
        }
    }
}
