//! Bytecode parser.
//!
//! Analyzes bytecode instructions to generate an abstract syntax tree.
use super::ast::{Ident, Lit, LocalVar, Node, Stmt};
use super::{Op, Proto};
use crate::errors::{Error, Result};
use crate::lua40::ast::{Block, Syntax};

pub struct Parser<'a> {
    proto: &'a Proto,

    /// Stack that mimics the operand stack used in the virtual machine.
    ///
    /// The [Ip] points to the bytecode instruction that pushed the
    /// slot item onto the stack.
    stack: Vec<Ip>,

    /// Space for the syntax tree nodes that are being built.
    ///
    /// This buffer has the same number of elements as the fucntion's
    /// bytecode buffer. Each node corresponds to an instruction.
    nodes: Box<[Option<Node>]>,

    /// Stack offset where local variables end.
    local_end: u32,
}

/// Instruction pointer.
///
/// Acts as the identifier for an instruction within the current function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Ip(u32);

// ============================================================================

impl<'a> Parser<'a> {
    pub fn new(root: &'a Proto) -> Self {
        Self {
            proto: root,
            stack: vec![],
            nodes: (0..root.code.len()).into_iter().map(|_| None).collect(),
            local_end: 0,
        }
    }

    pub fn parse(&mut self) -> Result<Syntax> {
        println!("parse");

        let iter = self
            .proto
            .ops
            .iter()
            .enumerate()
            .map(|(i, o)| (Ip(i as u32), o));

        for (ip, op) in iter {
            match op {
                Op::End => break,
                Op::Return { .. } => { /* todo */ }
                Op::Call { .. } => { /* todo */ }
                Op::PushInt { value } => {
                    self.parse_push_int(ip, *value)?;
                }
                Op::GetLocal { stack_offset } => {
                    self.parse_get_local(ip, *stack_offset)?;
                }
                Op::GetGlobal { .. } => { /* todo */ }
                Op::Add => { /* todo */ }
            }

            println!("stack: {:?}", self.stack);
            println!("nodes: {:?}", self.nodes);
        }

        let block = Block {
            nodes: self
                .nodes
                .iter_mut()
                .filter_map(|node| node.take())
                .collect(),
        };

        Ok(Syntax {
            root: block,
            debug: (),
        })
    }
}

impl<'a> Parser<'a> {
    fn parse_push_int(&mut self, ip: Ip, value: i32) -> Result<()> {
        // Pushes a constant integer into the stack top.
        self.stack.push(ip);

        // Integer literal in code.
        self.nodes[ip.as_usize()] = Some(Lit::Int(value).into());

        Ok(())
    }

    /// Parse a [Op::GetLocal] instruction.
    fn parse_get_local(&mut self, ip: Ip, stack_offset: u32) -> Result<()> {
        // Because the stack slot is being trated as a local variable, we
        // can check how it was written and possibly promote that syntax from
        // an expression into a local variable declaration statement.
        let node_ip = self.stack[stack_offset as usize];
        self.promote_local_var(node_ip)?;

        // Copies the value from the local variable's slot onto the stack top.
        self.stack.push(ip);

        Ok(())
    }
}

impl<'a> Parser<'a> {
    /// Promotes the syntax node the given instruction into a local variable declaration.
    fn promote_local_var(&mut self, ip: Ip) -> Result<()> {
        // If the stack slot is not a local variable declaration,
        // then promote it.
        //
        // Local variable declarations at the start of the function
        // may have their OP_SETLOCAL instructions removed as an
        // optimsation.
        if let Some(node) = &self.nodes[ip.as_usize()] {
            if !node.is_local_var() {
                let node = self.nodes[ip.as_usize()].take().unwrap();

                match node {
                    Node::Stmt(_) => {
                        return Error::new_parser(
                            "a statement cannot be turned into a local variable declaration",
                        )
                        .into()
                    }
                    Node::Expr(rhs) => {
                        // TODO: Generate name
                        let name = Ident::new("a");
                        let new_node = Node::Stmt(Stmt::LocalVar(LocalVar { name, rhs }));
                        self.nodes[ip.as_usize()] = Some(new_node);
                        self.local_end += 1;
                    }
                }
            }
        }

        Ok(())
    }
}

impl Ip {
    fn as_usize(self) -> usize {
        self.0 as usize
    }
}
