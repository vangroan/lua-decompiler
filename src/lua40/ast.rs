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
    Partial(Partial),
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
    Assign(Box<Assign>),
    Call(Box<Call>),
    Block(Block),
    If(IfBlock),
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

#[derive(Debug)]
pub struct Assign {
    pub name: Ident,
    pub rhs: Expr,
}

/// `if` conditional block statement.
#[derive(Debug)]
pub struct IfBlock {
    pub head: CondExpr,
    pub then: Block,
    pub else_: Option<Block>,
}

#[derive(Debug)]
pub enum CondExpr {
    Unary { op: (), rhs: Expr },
    Binary { op: CondOp, lhs: Expr, rhs: Expr },
}

/// Conditional operators.
#[derive(Debug)]
pub enum CondOp {
    Ne, // ~=
    Eq, // ==
    Lt, // <
    Le, // <=
    Gt, // >
    Ge, // >=
}

// ----------------------------------------------------------------------------
// Partials
// ----------------------------------------------------------------------------

/// A partially built statement.
#[derive(Debug)]
pub enum Partial {
    IfHead(Box<IfHead>),
    WhileHead,
    ForHead,
}

/// Header for an `if` conditional statement.
#[derive(Debug)]
pub struct IfHead {
    pub expr: CondExpr,
}

// ----------------------------------------------------------------------------
// Expressions
// ----------------------------------------------------------------------------

#[derive(Debug)]
pub enum Expr {
    /// Variable access by name.
    Access(Ident),
    Literal(Lit),
    Binary(Box<BinExpr>),
    Call(Box<Call>),
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

#[derive(Debug)]
pub struct Call {
    pub name: Expr,
    pub args: Vec<Expr>,
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

    pub fn into_expr(self) -> Option<Expr> {
        match self {
            Node::Expr(expr) => Some(expr),
            _ => None,
        }
    }

    pub fn into_partial(self) -> Option<Partial> {
        match self {
            Node::Partial(partial) => Some(partial),
            _ => None,
        }
    }
}

impl Ident {
    pub fn new(text: impl ToString) -> Self {
        Self {
            text: text.to_string(),
        }
    }

    pub fn as_str(&self) -> &str {
        self.text.as_str()
    }
}

impl fmt::Display for Ident {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self.text.as_str(), f)
    }
}

impl From<Ident> for Node {
    fn from(ident: Ident) -> Self {
        Node::Expr(Expr::Access(ident))
    }
}

impl From<IfHead> for Node {
    fn from(if_head: IfHead) -> Self {
        Node::Partial(Partial::IfHead(Box::new(if_head)))
    }
}

impl From<Lit> for Node {
    fn from(lit: Lit) -> Self {
        Node::Expr(Expr::Literal(lit))
    }
}

impl From<BinExpr> for Node {
    fn from(bin_expr: BinExpr) -> Self {
        Node::Expr(Expr::Binary(Box::new(bin_expr)))
    }
}

impl From<Call> for Node {
    fn from(call: Call) -> Self {
        Node::Expr(Expr::Call(Box::new(call)))
    }
}

impl Node {
    /// Checks whether the statement is partially built.
    #[inline(always)]
    pub fn is_partial(&self) -> bool {
        matches!(self, Node::Partial(_))
    }

    /// Checks whether the statement is completely built.
    #[inline(always)]
    pub fn is_complete(&self) -> bool {
        !self.is_partial()
    }
}
