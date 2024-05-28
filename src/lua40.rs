#![allow(dead_code)]
use byteorder::{LittleEndian, ReadBytesExt};
use std::ffi::CString;
use std::io::{Cursor, Read};

use crate::errors::{Error, Result};
use crate::reader::{Endianess, Number};

const LUA_VERSION: u8 = 0x40;
const ID_CHUNK: u8 = 27;
const SIGNATURE: &str = "Lua";
const TEST_NUMBER: f64 = 3.14159265358979323846E8;

/// As per `lopcode.h`
#[derive(Debug)]
pub enum Op {
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

/// Lua 4.0 bytecode chunk decoder.
pub struct Decoder<'a> {
    code: &'a [u8],
    cursor: Cursor<&'a [u8]>,
}

impl<'a> Decoder<'a> {
    pub fn new(code: &'a [u8]) -> Self {
        Self {
            code,
            cursor: Cursor::new(code),
        }
    }

    pub fn decode(&mut self) -> Result<()> {
        self.read_bytemark()?;
        self.read_signature()?;
        self.read_version()?;
        let endianess = self.read_endianess()?;
        let size_int = self.cursor.read_u8()?;
        let size_t = self.cursor.read_u8()?;
        let size_instr1 = self.cursor.read_u8()?;
        let size_instr_args = self.cursor.read_u8()?;
        let size_op = self.cursor.read_u8()?;
        let size_b = self.cursor.read_u8()?;
        let size_number = self.cursor.read_u8()?;

        println!("endianess: {endianess:?}; int: {size_int}B; size_t: {size_t}B; instruction: {size_instr1}B; args: {size_instr_args}b; op: {size_op}b; B: {size_b}b; Number: {size_number}B");

        let number = match size_number {
            4 => Number::F32,
            8 => Number::F64,
            _ => return Error::new_decoder("unknown number size: {size_number}").into(),
        };
        self.check_number_format(number, endianess)?;
        println!("number format check passed");

        // Top level function
        self.read_function(size_t as usize, endianess)?;

        Ok(())
    }
}

impl<'a> Decoder<'a> {
    fn read_bytemark(&mut self) -> Result<()> {
        let bytemark = self.cursor.read_u8()?;
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
        let version = self.cursor.read_u8()?;
        if version == LUA_VERSION {
            Ok(version)
        } else {
            Error::new_decoder("expected Lua version 4.0(0x40), found: {version:02x}").into()
        }
    }

    fn read_endianess(&mut self) -> Result<Endianess> {
        // Endianess is determined in C by casting a 32-bit
        // integer to a 8-bit character.
        //
        //  int x = 1;
        //  char endian = *(char *)&x;
        let mark = self.cursor.read_u8()?;
        if mark == 0 {
            Ok(Endianess::Big)
        } else {
            Ok(Endianess::Little)
        }
    }

    fn check_number_format(&mut self, number: Number, _endianess: Endianess) -> Result<()> {
        match number {
            Number::F32 => {
                if self.cursor.read_f32::<LittleEndian>()? == TEST_NUMBER as f32 {
                    Ok(())
                } else {
                    Error::new_decoder("unknown f32 number format").into()
                }
            }
            Number::F64 => {
                let f = self.cursor.read_f64::<LittleEndian>()?;
                println!("f: {f}");
                if f == TEST_NUMBER {
                    Ok(())
                } else {
                    Error::new_decoder("unknown f64 number format").into()
                }
            }
        }
    }

    fn read_function(&mut self, size_t: usize, endianess: Endianess) -> Result<()> {
        let source = self.read_string(size_t, endianess)?;
        println!("source: {source}");

        todo!()
    }

    fn read_string(&mut self, size_t: usize, endianess: Endianess) -> Result<String> {
        // TODO: dynamic size_t and endianess
        let len = self.read_size_t(size_t, endianess)?;
        println!("string len: {len}");
        let mut buf = vec![0u8; len];
        self.cursor.read_exact(&mut buf)?;
        let c_string =
            CString::from_vec_with_nul(buf).map_err(|err| Error::new_decoder(format!("{err}")))?;
        let string = c_string
            .into_string()
            .map_err(|err| Error::new_decoder(format!("{err}")))?;
        Ok(string)
    }

    fn read_size_t(&mut self, size_t: usize, _endianess: Endianess) -> Result<usize> {
        match size_t {
            4 => Ok(self.cursor.read_u32::<LittleEndian>()? as usize),
            8 => Ok(self.cursor.read_u64::<LittleEndian>()? as usize),
            _ => Error::new_decoder(format!("unknown size_t: {size_t}")).into(),
        }
    }
}
