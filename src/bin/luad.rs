use std::fs;

use clap::Parser;

use lua_decompiler::lua40;

#[derive(Parser, Debug)]
struct Cli {
    file: String,
}

fn main() {
    let args = Cli::parse();

    let code = fs::read(args.file).expect("failed to read file");
    let mut decoder = lua40::Decoder::new(&code);
    // TODO: Should decode return a chunk (with header info)?
    let main_proto = decoder.decode().expect("failed to decode");
    let mut parser = lua40::Parser::new(&main_proto);
    let syntax = parser.parse().expect("failed to parse");
    let mut scribe = lua40::Scribe::new();
    let mut buf = String::new();
    scribe.fmt_syntax(&mut buf, &syntax).expect("scribe failed");
    println!("output:\n{buf}");
}
