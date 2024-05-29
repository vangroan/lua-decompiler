//! Abstract syntax tree.
use std::fmt::{self, Formatter};

/// Abstract syntax tree.
#[derive(Debug)]
pub struct Syntax {
    pub root: Block,
    pub debug: (),
}

/// Block of statements.
#[derive(Debug)]
pub struct Block {
    // FIXME: Should this be statements?
    pub nodes: Vec<Node>,
}

/// Syntax Node.
#[derive(Debug)]
pub enum Node {
    Stmt(Stmt),
    Expr(Expr),
}

#[derive(Debug)]
pub struct Ident {
    text: String,
}

// ----------------------------------------------------------------------------
// Statements
// ----------------------------------------------------------------------------

#[derive(Debug)]
pub enum Stmt {
    LocalVar(LocalVar),
}

/// Local variable declaration.
///
/// ```lua
/// local {name} = {rhs}
/// ```
#[derive(Debug)]
pub struct LocalVar {
    pub name: Ident,
    pub rhs: Expr,
}

// ----------------------------------------------------------------------------
// Expressions
// ----------------------------------------------------------------------------

#[derive(Debug)]
pub enum Expr {
    Literal(Lit),
    Binary(Box<BinExpr>),
}

/// Literal value.
#[derive(Debug)]
pub enum Lit {
    Int(i32),
    Num(f64),
    Str(String),
}

#[derive(Debug)]
pub struct BinExpr {
    pub op: BinOp,
    pub lhs: Expr,
    pub rhs: Expr,
}

#[derive(Debug)]
pub enum BinOp {
    Add,
}

// ============================================================================
// Functions
// ============================================================================

impl Node {
    /// Check whether the node is a local variable declaration statement.
    #[inline(always)]
    pub fn is_local_var(&self) -> bool {
        matches!(self, Node::Stmt(Stmt::LocalVar(_)))
    }
}

impl Ident {
    pub fn new(text: impl ToString) -> Self {
        Self {
            text: text.to_string(),
        }
    }
}

impl fmt::Display for Ident {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.text.as_str(), f)
    }
}

impl From<Lit> for Node {
    fn from(lit: Lit) -> Self {
        Node::Expr(Expr::Literal(lit))
    }
}
