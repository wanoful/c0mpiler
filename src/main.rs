use std::{fs, io::{stdin, Read}, process::exit};

use clap::Parser;
use c0mpiler::{
    ast::{Crate, Eatable},
    ir::layout::TargetDataLayout,
    irgen::IRGenerator,
    lexer::{Lexer, TokenBuffer},
    mir::lower::{LowerOptions, RV32Lowerer, RV64Lowerer},
    semantics::analyzer::SemanticAnalyzer,
};

const PRELUDE: &str = include_str!("../tests/prelude.c");

#[derive(Parser)]
#[command(name = "c0mpiler", about = "A compiler for the .rx language")]
struct Args {
    /// Input file (reads from stdin if omitted)
    input: Option<String>,

    /// Target architecture
    #[arg(short, long, default_value = "rv32", value_parser = ["rv32", "rv64"])]
    target: String,

    /// Output format
    #[arg(short, long, default_value = "asm", value_parser = ["ir", "asm"])]
    emit: String,

    /// Suppress prelude output on stderr
    #[arg(long)]
    no_prelude: bool,
}

fn main() {
    let args = Args::parse();

    let src = match &args.input {
        Some(path) => fs::read_to_string(path).unwrap_or_else(|e| {
            eprintln!("error reading {}: {e}", path);
            exit(1);
        }),
        None => {
            let mut s = String::new();
            stdin().read_to_string(&mut s).unwrap();
            s
        }
    };

    let lexer = Lexer::new(&src);
    let buffer = TokenBuffer::new(lexer).unwrap_or_else(|e| {
        eprintln!("lexer error: {e:?}");
        exit(2);
    });
    let mut iter = buffer.iter();
    let krate = Crate::eat(&mut iter).unwrap_or_else(|e| {
        eprintln!("parse error: {e:?}");
        exit(3);
    });

    let (analyzer, semantic_result) = SemanticAnalyzer::visit(&krate);
    semantic_result.unwrap_or_else(|e| {
        eprintln!("semantic error: {e:?}");
        exit(4);
    });

    let target_layout = match args.target.as_str() {
        "rv64" => TargetDataLayout::rv64(),
        _ => TargetDataLayout::rv32(),
    };

    let mut ir_gen = IRGenerator::new(&analyzer, target_layout);
    ir_gen.visit(&krate);
    ir_gen.opt_all();

    match args.emit.as_str() {
        "ir" => {
            print!("{}", ir_gen.print());
        }
        "asm" | _ => {
            let module = ir_gen.module();
            let lower_options = LowerOptions {
                lower_function_bodies: true,
                need_branch_relaxation: true,
                optimize_fallthroughs: true,
                optimize_peephole: true,
            };

            let asm = match args.target.as_str() {
                "rv64" => {
                    let mut lowerer = RV64Lowerer::with_options(lower_options);
                    lowerer.lower_module(&module).unwrap_or_else(|e| {
                        eprintln!("lowering error: {e}");
                        exit(5);
                    })
                    .to_string()
                }
                _ => {
                    let mut lowerer = RV32Lowerer::with_options(lower_options);
                    lowerer.lower_module(&module).unwrap_or_else(|e| {
                        eprintln!("lowering error: {e}");
                        exit(5);
                    })
                    .to_string()
                }
            };
            print!("{asm}");
        }
    }

    if !args.no_prelude {
        eprint!("{PRELUDE}");
    }
}
