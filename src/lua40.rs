//! Lua 4.0 Decompiler.
//!
//! # Opcodes
//!
//! ```text
//!       26     6
//!  __________ ____
//! |    U     | Op |
//! |    S   |s| Op |
//! |  A  | B  | Op |
//! ```

#![allow(dead_code)]
use byteorder::ReadBytesExt;
use std::ffi::CString;
use std::fmt::{self, Formatter};
use std::io::{Cursor, Read};

use crate::errors::{Error, Result};
use crate::reader::{Endian, NumberType};

mod ast;
mod parser;
mod scribe;

pub use parser::Parser;
pub use scribe::Scribe;

const LUA_VERSION: u8 = 0x40;
const ID_CHUNK: u8 = 27;
const SIGNATURE: &str = "Lua";
const TEST_NUMBER: f64 = 3.14159265358979323846E8;

/// As per `lopcode.h`
#[derive(Debug)]
pub enum Opcode {
    End = 0,
    Return,

    Call,
    TailCall,

    PushNil,
    Pop,

    PushInt,
    PushString,
    PushNum,
    PushNegNum,

    PushValue,

    GetLocal,
    GetGlobal,

    GetTable,
    GetDotted,
    GetIndexed,
    PushSelf,

    CreateTable,

    SetLocal,
    SetGlobal,
    SetTable,

    SetList,
    SetMap,

    Add = 23,
    AddI,
    Sub,
    Mult,
    Div,
    Pow,
    Concat,
    Minus,
    Not,

    JumpNe,
    JumpEq,
    JumpLt,
    JumpLe,
    JumpGt,
    JumpGe,

    JumpTrue,
    JumpFalse,
    JumpOnTrue,
    JumpOnFalse,
    Jump,

    PushNilJump,

    ForPrep,
    ForLoop,

    LForPrep,
    LForLoop,

    Closure = 48,
}

#[derive(Debug, Clone)]
enum Op {
    End,
    /// Return from the current activation frame.
    ///
    /// Argument `U` is the numbeber of result values left on the stack.
    Return {
        results: u32,
    },

    /// Call Lua or C function.
    ///
    /// Argument `A` is the stack offset relative to the callee's stack base.
    ///
    /// Argument `B` is the number of result values left on the stack. When it's 255 (unsigned)
    /// in bytecode it means the function has multiple returns.
    Call {
        stack_offset: u32,
        results: u32,
    },

    Pop {
        n: u32,
    },

    /// Push an integer constant onto the stack.
    ///
    /// Argument `S` is the inlined signed integer value.
    PushInt {
        value: i32,
    },

    /// Copy the local variable from stack index `U` to the top of the stack.
    GetLocal {
        stack_offset: u32,
    },
    /// Copy a global variable to the top of the stack.
    ///
    /// Argument `U` is the index of the string constant that acts as the key.
    GetGlobal {
        string_id: u32,
    },

    SetLocal {
        stack_offset: u32,
    },

    Add,

    JumpLe {
        ip: i32,
    },
}

#[derive(Debug)]
struct Header {
    version: u8,
    endianess: Endian,
    size_int: u8,
    size_t: u8,
    size_instr: u8,
    size_instr_arg: u8,
    size_op: u8,
    size_b: u8,
    number_type: NumberType,
}

/// Function prototype.
#[derive(Debug)]
pub struct Proto {
    code: Box<[u32]>,
    ops: Box<[Op]>,
    source: String,
    line_defined: u32,
    num_params: u32,
    is_vararg: bool,
    max_stack: u32,
    locals: Box<[Local]>,
    constants: Constants,
    lines: Box<[u32]>,
}

/// Debug information for local variable.
#[derive(Debug)]
struct Local {
    varname: String,
    /// Point where variable is live.
    startpc: u32,
    /// Point where variable is dead.
    endpc: u32,
}

#[derive(Debug)]
struct Constants {
    strings: Box<[String]>,
    numbers: Box<[f64]>,
    protos: Box<[Proto]>,
}

/// Lua 4.0 bytecode chunk decoder.
pub struct Decoder<'a> {
    code: &'a [u8],
    cursor: Cursor<&'a [u8]>,
    header: Header,
}

// ============================================================================

/// Creates a mask with `n` 1 bits at position `p`.
macro_rules! mask1 {
    ($n:expr, $p:expr) => {
        (!(!0u32 << $n) << $p)
    };
}

// ============================================================================

impl TryFrom<u32> for Opcode {
    type Error = Error;

    fn try_from(value: u32) -> Result<Self> {
        use Opcode::*;

        Ok(match value {
            0 => End,
            1 => Return,
            2 => Call,
            3 => TailCall,
            4 => PushNil,
            5 => Pop,
            6 => PushInt,
            7 => PushString,
            8 => PushNum,
            9 => PushNegNum,
            10 => PushValue,
            11 => GetLocal,
            12 => GetGlobal,
            13 => GetTable,
            14 => GetDotted,
            15 => GetIndexed,
            16 => PushSelf,
            17 => CreateTable,
            18 => SetLocal,
            19 => SetGlobal,
            20 => SetTable,
            21 => SetList,
            22 => SetMap,
            23 => Add,
            24 => AddI,
            25 => Sub,
            26 => Mult,
            27 => Div,
            28 => Pow,
            29 => Concat,
            30 => Minus,
            31 => Not,
            32 => JumpNe,
            33 => JumpEq,
            34 => JumpLt,
            35 => JumpLe,
            36 => JumpGt,
            37 => JumpGe,
            38 => JumpTrue,
            39 => JumpFalse,
            40 => JumpOnTrue,
            41 => JumpOnFalse,
            42 => Jump,
            43 => PushNilJump,
            44 => ForPrep,
            45 => ForLoop,
            46 => LForPrep,
            47 => LForLoop,
            48 => Closure,
            _ => return Error::new_decoder("unknown opcode: 0x{value:02x}").into(),
        })
    }
}

impl Header {
    /// Size of instruction argument `U` (unsigned int).
    fn size_u(&self) -> u32 {
        self.size_instr_arg as u32 - self.size_op as u32
    }

    /// Max value of instruction argument `U` (unsigned int).
    fn max_arg_u(&self) -> u32 {
        (1 << self.size_u()) - 1
    }

    /// Max value of instruction argument `S` (signed int).
    fn max_arg_s(&self) -> i32 {
        // 1 bit taken up by sign.
        self.max_arg_u() as i32 >> 1
    }

    /// Position of instruction argument `A`.
    fn pos_arg_a(&self) -> u32 {
        self.size_op as u32 + self.size_b as u32
    }

    /// Position of instruction argument `B`.
    fn pos_arg_b(&self) -> u32 {
        self.size_op as u32
    }

    /// Size of instruction argument `A`,
    fn size_a(&self) -> u32 {
        self.size_instr_arg as u32 - (self.size_op as u32 + self.size_b as u32)
    }

    /// Max value of instruction argument `A`,
    fn max_arg_a(&self) -> u32 {
        (1 << self.size_a()) - 1
    }

    /// Max value of instruction argument `B`,
    fn max_arg_b(&self) -> u32 {
        (1 << self.size_b) - 1
    }
}

impl Default for Header {
    fn default() -> Self {
        Self {
            version: LUA_VERSION,
            endianess: Endian::Little,
            size_int: 0,
            size_t: 0,
            size_instr: 0,
            size_instr_arg: 0,
            size_op: 0,
            size_b: 0,
            number_type: NumberType::F64,
        }
    }
}

impl fmt::Display for Header {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let Self {
            version,
            endianess,
            size_int,
            size_t,
            size_instr,
            size_instr_arg,
            size_op,
            size_b,
            number_type,
        } = self;
        write!(f, "version: {version:02x}, endianess: {endianess:?}; int: {size_int}B; size_t: {size_t}B; instruction: {size_instr}B; args: {size_instr_arg}bits; opcode: {size_op}bits; B: {size_b}bits; Number: {number_type:?}")
    }
}

impl<'a> Decoder<'a> {
    pub fn new(code: &'a [u8]) -> Self {
        Self {
            code,
            cursor: Cursor::new(code),
            header: Header::default(),
        }
    }

    pub fn decode(&mut self) -> Result<Proto> {
        self.read_bytemark()?;
        self.read_signature()?;
        self.header = Header {
            version: self.read_version()?,
            endianess: self.read_endianess()?,
            size_int: self.read_u8()?,
            size_t: self.read_u8()?,
            size_instr: self.read_u8()?,
            size_instr_arg: self.read_u8()?,
            size_op: self.read_u8()?,
            size_b: self.read_u8()?,
            number_type: {
                let size_number = self.read_u8()?;
                match size_number {
                    4 => NumberType::F32,
                    8 => NumberType::F64,
                    _ => return Error::new_decoder("unknown number size: {size_number}").into(),
                }
            },
        };

        // println!("endianess: {endianess:?}; int: {size_int}B; size_t: {size_t}B; instruction: {size_instr1}B; args: {size_instr_args}b; op: {size_op}b; B: {size_b}b; Number: {size_number}B");
        println!("{}", self.header);

        self.check_number_format(self.header.number_type, self.header.endianess)?;
        println!("number format check passed");

        // Top level function
        let proto = self.read_function()?;

        println!("{proto:#?}");

        Ok(proto)
    }
}

impl<'a> Decoder<'a> {
    fn read_bytemark(&mut self) -> Result<()> {
        let bytemark = self.read_u8()?;
        if bytemark == ID_CHUNK {
            Ok(())
        } else {
            Error::new_decoder("chunk bytemark must be 'Esc'(27), found: {bytemark}").into()
        }
    }

    fn read_signature(&mut self) -> Result<()> {
        let mut buf = [0u8; SIGNATURE.len()];
        self.cursor.read_exact(&mut buf)?;
        if buf == SIGNATURE.as_bytes() {
            Ok(())
        } else {
            Error::new_decoder("bad signature").into()
        }
    }

    /// Returns version.
    fn read_version(&mut self) -> Result<u8> {
        let version = self.read_u8()?;
        if version == LUA_VERSION {
            Ok(version)
        } else {
            Error::new_decoder("expected Lua version 4.0(0x40), found: {version:02x}").into()
        }
    }

    fn read_endianess(&mut self) -> Result<Endian> {
        // Endianess is determined in C by casting a 32-bit
        // integer to a 8-bit character.
        //
        //  int x = 1;
        //  char endian = *(char *)&x;
        let mark = self.read_u8()?;
        if mark == 0 {
            Ok(Endian::Big)
        } else {
            Ok(Endian::Little)
        }
    }

    fn check_number_format(&mut self, number: NumberType, _endianess: Endian) -> Result<()> {
        match number {
            NumberType::F32 => {
                if self.read_f32()? == TEST_NUMBER as f32 {
                    Ok(())
                } else {
                    Error::new_decoder("unknown f32 number format").into()
                }
            }
            NumberType::F64 => {
                let f = self.read_f64()?;
                println!("f: {f}");
                if f == TEST_NUMBER {
                    Ok(())
                } else {
                    Error::new_decoder("unknown f64 number format").into()
                }
            }
        }
    }

    fn read_function(&mut self) -> Result<Proto> {
        let source = self.read_string()?;
        let line_defined = self.read_u32()?;
        let num_params = self.read_u32()?;
        let is_vararg = self.read_u8()? != 0;
        let max_stack = self.read_u32()?;

        let locals = self.read_locals()?;
        let lines = self.read_lines()?;
        let constants = self.read_constants()?;
        let code = self.read_code()?;

        let mut ops: Box<[Op]> = (0..code.len()).into_iter().map(|_| Op::End).collect();
        for (index, instr) in code.iter().cloned().enumerate() {
            ops[index] = self.decode_op(instr)?;
        }

        assert_eq!(code.len(), ops.len());

        Ok(Proto {
            code,
            ops,
            source,
            line_defined,
            num_params,
            is_vararg,
            max_stack,
            locals,
            constants,
            lines,
        })
    }

    fn read_string(&mut self) -> Result<String> {
        // TODO: dynamic size_t and endianess
        let len = self.read_size_t()?;
        let mut buf = vec![0u8; len];
        self.cursor.read_exact(&mut buf)?;
        let c_string =
            CString::from_vec_with_nul(buf).map_err(|err| Error::new_decoder(format!("{err}")))?;
        let string = c_string
            .into_string()
            .map_err(|err| Error::new_decoder(format!("{err}")))?;
        Ok(string)
    }

    fn read_size_t(&mut self) -> Result<usize> {
        match self.header.size_t {
            2 => Ok(self.read_u16()? as usize),
            4 => Ok(self.read_u32()? as usize),
            8 => Ok(self.read_u64()? as usize),
            _ => Error::new_decoder(format!("unknown size_t: {}", self.header.size_t)).into(),
        }
    }

    fn read_locals(&mut self) -> Result<Box<[Local]>> {
        let n = self.read_u32()?;
        let mut locals = vec![];
        for _ in 0..n {
            locals.push(Local {
                varname: self.read_string()?,
                startpc: self.read_u32()?,
                endpc: self.read_u32()?,
            });
        }
        Ok(locals.into_boxed_slice())
    }

    fn read_lines(&mut self) -> Result<Box<[u32]>> {
        let n = self.read_u32()?;
        let mut lines = vec![];
        for _ in 0..n {
            lines.push(self.read_u32()?);
        }
        Ok(lines.into_boxed_slice())
    }

    fn read_constants(&mut self) -> Result<Constants> {
        let mut strings = vec![];
        let mut numbers = vec![];
        let mut protos = vec![];

        for _ in 0..self.read_u32()? {
            strings.push(self.read_string()?);
        }

        for _ in 0..self.read_u32()? {
            numbers.push(self.read_f64()?);
        }

        for _ in 0..self.read_u32()? {
            protos.push(self.read_function()?);
        }

        Ok(Constants {
            strings: strings.into_boxed_slice(),
            numbers: numbers.into_boxed_slice(),
            protos: protos.into_boxed_slice(),
        })
    }

    fn read_code(&mut self) -> Result<Box<[u32]>> {
        let mut code = vec![];

        for _ in 0..self.read_u32()? {
            code.push(self.read_u32()?);
        }

        Ok(code.into_boxed_slice())
    }

    fn decode_op(&self, op: u32) -> Result<Op> {
        use Opcode::*;

        let Header { size_op, .. } = self.header;
        let opcode = Opcode::try_from(op & mask1!(size_op, 0))?;
        let arg_u = op >> size_op;
        let arg_s = arg_u as i32 - self.header.max_arg_s();
        let arg_a = op >> self.header.pos_arg_a();
        let arg_b = (op >> self.header.pos_arg_b()) & self.header.max_arg_b();

        let op = match opcode {
            End => Op::End,
            Return => Op::Return { results: arg_u },

            Call => Op::Call {
                stack_offset: arg_a,
                results: arg_b,
            },
            TailCall => todo!(),

            PushNil => todo!(),
            Pop => Op::Pop { n: arg_u },

            PushInt => Op::PushInt { value: arg_s },
            PushString => todo!(),
            PushNum => todo!(),
            PushNegNum => todo!(),

            PushValue => todo!(),

            GetLocal => Op::GetLocal {
                stack_offset: arg_u,
            },
            GetGlobal => Op::GetGlobal { string_id: arg_u },

            GetTable => todo!(),
            GetDotted => todo!(),
            GetIndexed => todo!(),
            PushSelf => todo!(),

            CreateTable => todo!(),

            SetLocal => Op::SetLocal {
                stack_offset: arg_u,
            },
            SetGlobal => todo!(),
            SetTable => todo!(),

            SetList => todo!(),
            SetMap => todo!(),

            Add => Op::Add,
            AddI => todo!(),
            Sub => todo!(),
            Mult => todo!(),
            Div => todo!(),
            Pow => todo!(),
            Concat => todo!(),
            Minus => todo!(),
            Not => todo!(),

            JumpNe => todo!(),
            JumpEq => todo!(),
            JumpLt => todo!(),
            JumpLe => Op::JumpLe { ip: arg_s },
            JumpGt => todo!(),
            JumpGe => todo!(),

            JumpTrue => todo!(),
            JumpFalse => todo!(),
            JumpOnTrue => todo!(),
            JumpOnFalse => todo!(),
            Jump => todo!(),

            PushNilJump => todo!(),

            ForPrep => todo!(),
            ForLoop => todo!(),

            LForPrep => todo!(),
            LForLoop => todo!(),

            Closure => todo!(),
        };

        Ok(op)
    }
}

impl<'a> Decoder<'a> {
    fn read_u8(&mut self) -> Result<u8> {
        Ok(self.cursor.read_u8()?)
    }

    fn read_u16(&mut self) -> Result<u16> {
        let mut buf = [0; std::mem::size_of::<u16>()];
        self.cursor.read_exact(&mut buf)?;
        match self.header.endianess {
            Endian::Little => Ok(u16::from_le_bytes(buf)),
            Endian::Big => Ok(u16::from_le_bytes(buf)),
        }
    }

    fn read_u32(&mut self) -> Result<u32> {
        let mut buf = [0; std::mem::size_of::<u32>()];
        self.cursor.read_exact(&mut buf)?;
        match self.header.endianess {
            Endian::Little => Ok(u32::from_le_bytes(buf)),
            Endian::Big => Ok(u32::from_le_bytes(buf)),
        }
    }

    fn read_u64(&mut self) -> Result<u64> {
        let mut buf = [0; std::mem::size_of::<u64>()];
        self.cursor.read_exact(&mut buf)?;
        match self.header.endianess {
            Endian::Little => Ok(u64::from_le_bytes(buf)),
            Endian::Big => Ok(u64::from_le_bytes(buf)),
        }
    }

    fn read_f32(&mut self) -> Result<f32> {
        let mut buf = [0; std::mem::size_of::<f32>()];
        self.cursor.read_exact(&mut buf)?;
        match self.header.endianess {
            Endian::Little => Ok(f32::from_le_bytes(buf)),
            Endian::Big => Ok(f32::from_le_bytes(buf)),
        }
    }

    fn read_f64(&mut self) -> Result<f64> {
        let mut buf = [0; std::mem::size_of::<f64>()];
        self.cursor.read_exact(&mut buf)?;
        match self.header.endianess {
            Endian::Little => Ok(f64::from_le_bytes(buf)),
            Endian::Big => Ok(f64::from_le_bytes(buf)),
        }
    }
}

struct ProtoDump<'a> {
    proto: &'a Proto,
}

impl<'a> fmt::Display for ProtoDump<'a> {
    fn fmt(&self, _f: &mut Formatter) -> fmt::Result {
        todo!()
    }
}
