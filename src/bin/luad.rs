use std::fs;

use clap::Parser;

use lua_decompiler::lua40::Decoder;

#[derive(Parser, Debug)]
struct Cli {
    file: String,
}

fn main() {
    let args = Cli::parse();

    let code = fs::read(args.file).expect("failed to read file");
    let mut decoder = Decoder::new(&code);
    decoder.decode().expect("failed to decode");
}
