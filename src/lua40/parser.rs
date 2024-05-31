//! Bytecode parser.
//!
//! Analyzes bytecode instructions to generate an abstract syntax tree.
use std::fmt::{self, Formatter};

use super::ast::{
    Assign, BinExpr, BinOp, Call, CondExpr, CondOp, Expr, Ident, IfHead, Lit, LocalVar, Node, Stmt,
};
use super::{Op, Proto};
use crate::errors::{Error, Result};
use crate::lua40::ast::{Block, IfBlock, Partial, Syntax};

const ASCII_CHARS: [u8; 26] = [
    'a' as u8, 'b' as u8, 'c' as u8, 'd' as u8, 'e' as u8, 'f' as u8, 'g' as u8, 'h' as u8,
    'i' as u8, 'j' as u8, 'k' as u8, 'l' as u8, 'm' as u8, 'n' as u8, 'o' as u8, 'p' as u8,
    'q' as u8, 'r' as u8, 's' as u8, 't' as u8, 'u' as u8, 'v' as u8, 'w' as u8, 'x' as u8,
    'y' as u8, 'z' as u8,
];

pub struct Parser<'a> {
    proto: &'a Proto,

    /// Stack that mimics the operand stack used in the virtual machine.
    ///
    /// The [Ip] points to the bytecode instruction that pushed the
    /// slot item onto the stack.
    stack: Vec<Ip>,

    /// Space for the syntax tree nodes that are being built.
    ///
    /// This buffer has the same number of elements as the function's
    /// bytecode buffer. Each node corresponds to an instruction.
    nodes: Box<[Option<Node>]>,

    /// Stack of block spans.
    blocks: Vec<BlockSpan>,

    /// Stack offset where local variables end.
    local_end: u32,

    /// Discovered local variables.
    ///
    /// When the chunk's debug information is stripped,
    /// we have to build up our own metadata for local variables.
    locals: Vec<Local>,

    /// namer for local variables.
    local_namer: Namer,
}

/// Instruction pointer.
///
/// Acts as the identifier for an instruction within the current function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Ip(u32);

#[derive(Debug)]
struct BlockSpan {
    /// Instruction where the block started.
    start: Ip,
    /// Instruction right after the last instruction in the block.
    end: Ip,
}

struct Local {
    name: String,
    stack_offset: u32,
    /// Flag indicating whether the declaration statement
    /// has been encountered.
    is_declared: bool,
}

struct Namer {
    /// Set of characters that can be used to generate names.
    chars: Box<[u8]>,
    cursor: usize,
    count: usize,
}

// ============================================================================

fn err_stack_underflow() -> Error {
    Error::new_parser("operand stack underflow")
}

fn err_expr_expected() -> Error {
    Error::new_parser("expected expression")
}

fn err_partial_expected() -> Error {
    Error::new_parser("expected partial statement")
}

fn err_node_none() -> Error {
    Error::new_parser("no syntax node for bytecode")
}

// ============================================================================

impl<'a> Parser<'a> {
    pub fn new(root: &'a Proto) -> Self {
        Self {
            proto: root,
            stack: vec![],
            nodes: (0..root.code.len()).into_iter().map(|_| None).collect(),
            blocks: vec![],
            local_end: 0,
            locals: vec![],
            local_namer: Namer::new(&ASCII_CHARS),
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
            println!("[{}] op: {op:?}", ip.as_usize() + 1);

            // If we reached the end marker of the block, wrap up
            // by collecting all the nodes in the block into a single node.
            if let Some(block) = self.blocks.last() {
                if ip == block.end {
                    self.end_block()?;
                }
            }

            match op {
                Op::End => break,
                Op::Return { .. } => { /* todo */ }
                Op::Call {
                    stack_offset,
                    results,
                } => self.parse_call(ip, *stack_offset, *results)?,
                Op::Pop { n } => self.parse_pop(*n)?,
                Op::PushInt { value } => self.parse_push_int(ip, *value)?,
                Op::GetLocal { stack_offset } => self.parse_get_local(ip, *stack_offset)?,
                Op::GetGlobal { string_id } => self.parse_get_global(ip, *string_id)?,
                Op::SetLocal { stack_offset } => self.parse_set_local(ip, *stack_offset)?,
                Op::Add => self.parse_binary_op(ip, BinOp::Add)?,
                Op::JumpLe { ip: dest_ip } => self.parse_jump_le(ip, *dest_ip)?,
            }

            println!("stack: {:?}", self.stack);
            println!("nodes: {:?}", self.nodes);
            println!("-------------")
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
    fn parse_call(&mut self, ip: Ip, stack_offset: u32, results: u32) -> Result<()> {
        // TODO: All the call semantics and how it interacts with the stack.

        // Truncate stack and leave results.
        let mut arg_ips = self.stack.split_off(stack_offset as usize);
        let name_ip = arg_ips.remove(0);

        // TODO: Multi return semantics (even possible for C calls?)
        for _ in 0..results {
            self.stack.push(ip);
        }

        let name = self.nodes[name_ip.as_usize()]
            .take()
            .ok_or_else(err_node_none)?
            .into_expr()
            .ok_or_else(err_expr_expected)?;
        let mut args = vec![];
        for arg_ip in arg_ips {
            args.push(
                self.nodes[arg_ip.as_usize()]
                    .take()
                    .ok_or_else(err_node_none)?
                    .into_expr()
                    .ok_or_else(err_expr_expected)?,
            );
        }

        let node: Node = if results == 0 {
            // When the call returns 0 results, it implies the function
            // was called as a statement.
            Node::Stmt(Stmt::Call(Box::new(Call { name, args })))
        } else {
            // When the call returns results, it was part of an expression.
            Node::Expr(Expr::Call(Box::new(Call { name, args })))
        };
        self.nodes[ip.as_usize()] = Some(node);

        Ok(())
    }

    fn parse_pop(&mut self, n: u32) -> Result<()> {
        // Removes 'n' slots from the stack.
        for _ in 0..n {
            self.stack.pop();
        }

        // Pop is implicit to remove locals at the end of a block,
        // so doesn't have any syntax to generate.

        Ok(())
    }

    fn parse_push_int(&mut self, ip: Ip, value: i32) -> Result<()> {
        // Pushes a constant integer into the stack top.
        self.stack.push(ip);

        // Integer literal in code.
        self.nodes[ip.as_usize()] = Some(Lit::Int(value).into());

        Ok(())
    }

    /// Parse a [Op::GetLocal] instruction.
    fn parse_get_local(&mut self, ip: Ip, stack_offset: u32) -> Result<()> {
        // Because the stack slot is now being treated as a local variable, we
        // can check how it was written and possibly promote that syntax from
        // an expression into a local variable declaration statement.
        let node_ip = self.stack[stack_offset as usize];
        self.promote_local_var(node_ip)?;

        // Copies the value from the local variable's slot onto the stack top.
        self.stack.push(ip);

        let local_name = self.get_local_var_name(stack_offset)?;
        self.nodes[ip.as_usize()] = Some(Ident::new(local_name).into());

        Ok(())
    }

    fn parse_get_global(&mut self, ip: Ip, string_id: u32) -> Result<()> {
        self.stack.push(ip);

        let global_name = self.get_global_var_name(string_id);
        self.nodes[ip.as_usize()] = Some(Ident::new(global_name).into());

        Ok(())
    }

    fn parse_set_local(&mut self, ip: Ip, stack_offset: u32) -> Result<()> {
        // An existing node that wrote the variable may be promoted to a variable declaration.
        let node_ip = self.stack[stack_offset as usize];
        self.promote_local_var(node_ip)?;

        // Value is 'moved' into the variable.
        let rhs_ip = self.stack.pop().ok_or_else(err_stack_underflow)?;
        let rhs_node = self.nodes[rhs_ip.as_usize()]
            .take()
            .ok_or_else(err_node_none)?
            .into_expr()
            .ok_or_else(err_expr_expected)?;

        let name = Ident::new(self.get_local_var_name(stack_offset)?);
        self.nodes[ip.as_usize()] = Some(Node::Stmt(Stmt::Assign(Box::new(Assign {
            name,
            rhs: rhs_node,
        }))));

        Ok(())
    }

    fn parse_binary_op(&mut self, ip: Ip, op: BinOp) -> Result<()> {
        let rhs_ip = self.stack.pop().ok_or_else(err_stack_underflow)?;
        let lhs_ip = self.stack.pop().ok_or_else(err_stack_underflow)?;

        let rhs = self.nodes[rhs_ip.as_usize()]
            .take()
            .ok_or_else(err_node_none)?
            .into_expr()
            .ok_or_else(err_expr_expected)?;
        let lhs = self.nodes[lhs_ip.as_usize()]
            .take()
            .ok_or_else(err_node_none)?
            .into_expr()
            .ok_or_else(err_expr_expected)?;

        self.nodes[ip.as_usize()] = Some(BinExpr { op, lhs, rhs }.into());

        self.stack.push(ip);

        Ok(())
    }

    fn parse_jump_le(&mut self, ip: Ip, dest_ip: i32) -> Result<()> {
        // Destination address is relative to the instruction following the current one.
        let end = (ip.0 as i32 + 1)
            .checked_add(dest_ip)
            .ok_or_else(|| Error::new_decoder("jump address overflow"))?;
        if end < 0 || end >= self.proto.code.len() as i32 {
            return Error::new_decoder("jump destination out of bounds").into();
        }
        self.start_block(ip, Ip(end as u32));

        // NOTE: Jump relative to the next ip
        // TODO: Generate if conditional statement and block nodes.
        let rhs_ip = self.stack.pop().ok_or_else(err_stack_underflow)?;
        let lhs_ip = self.stack.pop().ok_or_else(err_stack_underflow)?;

        let lhs = self.nodes[lhs_ip.as_usize()]
            .take()
            .ok_or_else(err_node_none)?
            .into_expr()
            .ok_or_else(err_expr_expected)?;
        let rhs = self.nodes[rhs_ip.as_usize()]
            .take()
            .ok_or_else(err_node_none)?
            .into_expr()
            .ok_or_else(err_expr_expected)?;

        self.nodes[ip.as_usize()] = Some(
            IfHead {
                expr: CondExpr::Binary {
                    op: CondOp::Le,
                    lhs,
                    rhs,
                },
            }
            .into(),
        );

        Ok(())
    }
}

impl<'a> Parser<'a> {
    /// Start a new block.
    fn start_block(&mut self, start: Ip, end: Ip) {
        self.blocks.push(BlockSpan { start, end })
    }

    fn end_block(&mut self) -> Result<()> {
        if let Some(BlockSpan { start, end }) = self.blocks.pop() {
            println!("end block ({start}, {end})");

            // TODO: if, while, for, do...
            // TODO: Conditional header
            // let _header = self.nodes[start.as_usize()].take().ok_or_else(err_node_none)?;

            let mut nodes = vec![];

            // Note that the ending instruction is exclusive.
            // The jump destination is the previous instruction.
            for maybe_node in &mut self.nodes[start.as_usize() + 1..end.as_usize()] {
                if let Some(node) = maybe_node.take() {
                    nodes.push(node);
                }
            }
            let body = Block { nodes };

            let head = self.nodes[start.as_usize()]
                .take()
                .ok_or_else(err_node_none)?
                .into_partial()
                .ok_or_else(err_partial_expected)?;
            match head {
                Partial::IfHead(if_head) => {
                    let IfHead { expr } = *if_head;
                    let node = Node::Stmt(Stmt::If(IfBlock {
                        head: expr,
                        then: body,
                        else_: None,
                    }));

                    // Place the new node into the header instruction.
                    self.nodes[start.as_usize()] = Some(node);
                }
                Partial::WhileHead => todo!(),
                Partial::ForHead => todo!(),
            }

            println!("stack: {:?}", self.stack);
            println!("nodes: {:?}", self.nodes);
            println!("-------------")
        }

        Ok(())
    }

    /// Promotes the syntax node the given instruction into a local variable declaration.
    ///
    /// Returns `true` if the node was promoted.
    fn promote_local_var(&mut self, ip: Ip) -> Result<bool> {
        // If the stack slot is not a local variable declaration,
        // then promote it.
        //
        // Local variable declarations at the start of the function
        // may have their OP_SETLOCAL instructions removed as an
        // optimsation.
        if let Some(node) = &self.nodes[ip.as_usize()] {
            // TODO: Consider the case where an expression assigned after declaration.
            if !node.is_local_var() {
                let node = self.nodes[ip.as_usize()].take().unwrap();

                match node {
                    Node::Expr(rhs) => {
                        // Generate a new name for the local variable.
                        // TODO: Detect conflict with globals or up-values.
                        let name = Ident::new(self.local_namer.next());
                        let new_node = Node::Stmt(Stmt::LocalVar(LocalVar { name, rhs }));
                        self.nodes[ip.as_usize()] = Some(new_node);
                        self.local_end += 1;
                        return Ok(true);
                    }
                    Node::Stmt(_) => {
                        return Error::new_parser(
                            "a statement cannot be turned into a local variable declaration",
                        )
                        .into()
                    }
                    Node::Partial(_) => {
                        return Error::new_parser(
                            "a partially built statement cannot be turned into a local variable declaration",
                        )
                        .into()
                    }
                }
            }
        }

        Ok(false)
    }

    fn get_local_var_name(&self, local_id: u32) -> Result<&str> {
        // TODO: Tracking local variables may require a dedicated Vec<Local> because this node migh tbe overwritten.
        let node_ip = self.stack[local_id as usize];
        match self.nodes[node_ip.as_usize()]
            .as_ref()
            .ok_or_else(err_node_none)?
        {
            Node::Stmt(stmt) => match stmt {
                Stmt::LocalVar(local_var) => Ok(local_var.name.as_str()),
                _ => Error::new_parser("unexpected statement in local variable node").into(),
            },
            Node::Expr(_) => {
                Error::new_parser("unexpected expression in local variable node").into()
            }
            Node::Partial(_) => {
                Error::new_parser("unexpected partial statement in local variable node").into()
            }
        }
    }

    fn get_global_var_name(&self, string_id: u32) -> &str {
        self.proto.constants.strings[string_id as usize].as_str()
    }

    /// Checks whether we have a record of the local variable
    /// at the given stack slot.
    fn has_local(&self, stack_offset: u32) -> bool {
        stack_offset as usize >= self.locals.len()
    }

    fn declare_local(&self, name: impl ToString, stack_offset: u32) -> Result<()> {
        todo!("declare local")
    }
}

impl Ip {
    fn as_usize(self) -> usize {
        self.0 as usize
    }
}

impl fmt::Display for Ip {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

impl Namer {
    fn new(char_set: &[u8]) -> Self {
        Self {
            chars: char_set
                .iter()
                .cloned()
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            cursor: 0,
            count: 0,
        }
    }

    fn next(&mut self) -> String {
        // Determine the length of the name to generate,
        // depending on whether we've wrapped the available character set.
        let len = self.count / self.chars.len();
        let mut buf = String::new();

        for i in 0..len + 1 {
            let c = self.chars[(self.count + i) % self.chars.len()];
            buf.push(c as char);
        }

        self.count += 1;

        buf
    }
}
