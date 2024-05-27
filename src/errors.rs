use std::fmt::{self, Formatter};

pub type Result<T> = std::result::Result<T, self::Error>;

#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
}

#[derive(Debug)]
pub enum ErrorKind {
    Decoder(String),
    Io(std::io::Error),
}

impl Error {
    pub fn new_decoder(message: impl ToString) -> Self {
        Error {
            kind: ErrorKind::Decoder(message.to_string()),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        use ErrorKind::*;

        match &self.kind {
            Decoder(msg) => write!(f, "decoder error: {msg}"),
            Io(err) => fmt::Display::fmt(err, f),
        }
    }
}

impl From<std::io::Error> for self::Error {
    fn from(err: std::io::Error) -> Self {
        Error {
            kind: ErrorKind::Io(err),
        }
    }
}

impl<T> Into<Result<T>> for Error {
    fn into(self) -> Result<T> {
        Err(self)
    }
}
