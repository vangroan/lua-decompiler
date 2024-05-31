//! Code generator for Lua syntax.
use std::fmt::Write as FmtWrite;

use super::ast::{
    Assign, BinExpr, BinOp, Block, Call, CondExpr, CondOp, Expr, Ident, IfBlock, Lit, LocalVar,
    Node, Stmt, Syntax,
};
use crate::errors::Result;

pub struct Scribe {
    level: u32,
}

impl Scribe {
    pub fn new() -> Self {
        Self { level: 0 }
    }

    pub fn fmt_syntax(&mut self, f: &mut impl FmtWrite, syntax: &Syntax) -> Result<()> {
        self.fmt_block(f, &syntax.root)
    }

    fn with_indent<F>(&mut self, func: F) -> Result<()>
    where
        F: FnOnce(&mut Self) -> Result<()>,
    {
        self.level += 1;
        func(self)?;
        self.level -= 1;
        Ok(())
    }

    fn fmt_indent(&mut self, f: &mut impl FmtWrite) -> Result<()> {
        for _ in 0..self.level {
            write!(f, "    ")?;
        }
        Ok(())
    }

    fn fmt_block(&mut self, f: &mut impl FmtWrite, block: &Block) -> Result<()> {
        for node in &block.nodes {
            self.fmt_indent(f)?;
            self.fmt_node(f, node)?;
        }

        Ok(())
    }

    fn fmt_node(&mut self, f: &mut impl FmtWrite, node: &Node) -> Result<()> {
        match node {
            Node::Stmt(stmt) => self.fmt_stmt(f, stmt),
            // FIXME: Some expressions are valid statements, like Call. Can we detect this and wrap them in stmt?
            Node::Expr(expr) => self.fmt_expr(f, expr),
            Node::Partial(_) => panic!("partially built statement"),
        }
    }

    fn fmt_stmt(&mut self, f: &mut impl FmtWrite, stmt: &Stmt) -> Result<()> {
        match stmt {
            Stmt::LocalVar(local_var) => self.fmt_local_var(f, local_var),
            Stmt::Call(call) => self.fmt_call(f, call),
            Stmt::Assign(assign) => self.fmt_assign(f, assign),
            Stmt::Block(block) => self.fmt_block_stmt(f, block),
            Stmt::If(if_block) => self.fmt_if_block(f, if_block),
        }
    }

    fn fmt_local_var(&mut self, f: &mut impl FmtWrite, local_var: &LocalVar) -> Result<()> {
        let LocalVar { name, rhs } = local_var;
        write!(f, "local {name} = ")?;
        self.fmt_expr(f, rhs)?;
        writeln!(f)?;
        Ok(())
    }

    fn fmt_expr(&mut self, f: &mut impl FmtWrite, expr: &Expr) -> Result<()> {
        match expr {
            Expr::Access(ident) => self.fmt_access(f, ident),
            Expr::Literal(lit) => self.fmt_lit(f, lit),
            Expr::Binary(bin_expr) => self.fmt_binary_expr(f, bin_expr),
            Expr::Call(call) => self.fmt_call(f, &*call),
        }
    }

    fn fmt_access(&mut self, f: &mut impl FmtWrite, ident: &Ident) -> Result<()> {
        write!(f, "{}", ident)?;
        Ok(())
    }

    fn fmt_lit(&self, f: &mut impl FmtWrite, lit: &Lit) -> Result<()> {
        match lit {
            Lit::Int(value) => write!(f, "{}", value)?,
            Lit::Num(_) => todo!(),
            Lit::Str(_) => todo!(),
        }
        Ok(())
    }

    fn fmt_binary_expr(&mut self, f: &mut impl FmtWrite, bin_expr: &BinExpr) -> Result<()> {
        self.fmt_expr(f, &bin_expr.lhs)?;
        write!(f, " ")?;

        match bin_expr.op {
            BinOp::Add => write!(f, "+")?,
        }

        write!(f, " ")?;
        self.fmt_expr(f, &bin_expr.rhs)?;

        Ok(())
    }

    fn fmt_call(&mut self, f: &mut impl FmtWrite, call: &Call) -> Result<()> {
        self.fmt_expr(f, &call.name)?;
        write!(f, "(")?;
        for (i, arg) in call.args.iter().enumerate() {
            if i != 0 {
                write!(f, ", ")?;
            }
            self.fmt_expr(f, arg)?;
        }
        write!(f, ")")?;
        Ok(())
    }

    fn fmt_assign(&mut self, f: &mut impl FmtWrite, assign: &Assign) -> Result<()> {
        let Assign { name, rhs } = assign;
        write!(f, "{name} = ")?;
        self.fmt_expr(f, rhs)?;
        writeln!(f)?;
        Ok(())
    }

    fn fmt_block_stmt(&mut self, f: &mut impl FmtWrite, block: &Block) -> Result<()> {
        writeln!(f, "do")?;
        self.with_indent(|scribe| scribe.fmt_block(f, block))?;
        writeln!(f, "end")?;
        Ok(())
    }

    fn fmt_if_block(&mut self, f: &mut impl FmtWrite, if_block: &IfBlock) -> Result<()> {
        //  head
        write!(f, "if ")?;
        self.fmt_cond_expr(f, &if_block.head)?;
        writeln!(f, " then")?;

        // body
        self.with_indent(|scribe| scribe.fmt_block(f, &if_block.then))?;
        if let Some(else_) = &if_block.else_ {
            writeln!(f, "else")?;
            self.with_indent(|scribe| scribe.fmt_block(f, else_))?;
        }

        writeln!(f, "end")?;
        Ok(())
    }

    fn fmt_cond_expr(&mut self, f: &mut impl FmtWrite, expr: &CondExpr) -> Result<()> {
        match expr {
            CondExpr::Unary { .. } => todo!("unary expression"),
            CondExpr::Binary { op, lhs, rhs } => {
                self.fmt_expr(f, lhs)?;
                write!(f, " ")?;

                match op {
                    CondOp::Ne => write!(f, "~=")?,
                    CondOp::Eq => write!(f, "==")?,
                    CondOp::Lt => write!(f, "<")?,
                    CondOp::Le => write!(f, "<=")?,
                    CondOp::Gt => write!(f, ">")?,
                    CondOp::Ge => write!(f, ">=")?,
                }

                write!(f, " ")?;
                self.fmt_expr(f, rhs)?;
            }
        }

        Ok(())
    }
}
