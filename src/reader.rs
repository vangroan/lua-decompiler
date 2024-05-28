#![allow(dead_code)]
use std::io::Cursor;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Endian {
    Little,
    Big,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumberType {
    F32,
    F64,
}

pub struct CodeReader<'a> {
    code: &'a [u8],
    cursor: Cursor<&'a [u8]>,
    size_int: usize,
    size_t: usize,
}
