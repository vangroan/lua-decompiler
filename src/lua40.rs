#![allow(dead_code)]
use byteorder::ReadBytesExt;
use std::ffi::CString;
use std::fmt::{self, Formatter};
use std::io::{Cursor, Read};

use crate::errors::{Error, Result};
use crate::reader::{Endian, NumberType};

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

    Add,
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

#[derive(Debug)]
enum Op {
    End,
    Return { u: u8 },
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
struct Proto {
    code: Box<[u32]>,
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

    pub fn decode(&mut self) -> Result<()> {
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

        println!("{proto:?}");

        Ok(())
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

        Ok(Proto {
            code,
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

    fn decode_op(&self, op: u32) -> () {
        let opcode = op & (self.header.size_op as u32 - 1);
        match opcode {
            End => {},
            Return => {},

            Call => {},
            TailCall => {},

            PushNil => {},
            Pop => {},

            PushInt => {},
            PushString => {},
            PushNum => {},
            PushNegNum => {},

            PushValue => {},

            GetLocal => {},
            GetGlobal => {},

            GetTable => {},
            GetDotted => {},
            GetIndexed => {},
            PushSelf => {},

            CreateTable => {},

            SetLocal => {},
            SetGlobal => {},
            SetTable => {},

            SetList => {},
            SetMap => {},

            Add => {},
            AddI => {},
            Sub => {},
            Mult => {},
            Div => {},
            Pow => {},
            Concat => {},
            Minus => {},
            Not => {},

            JumpNe => {},
            JumpEq => {},
            JumpLt => {},
            JumpLe => {},
            JumpGt => {},
            JumpGe => {},

            JumpTrue => {},
            JumpFalse => {},
            JumpOnTrue => {},
            JumpOnFalse => {},
            Jump => {},

            PushNilJump => {},

            ForPrep => {},
            ForLoop => {},

            LForPrep => {},
            LForLoop => {},

            Closure => {},
        }
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
