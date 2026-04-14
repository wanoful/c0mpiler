use std::io::{Read, stdin};

use c0mpiler::{
    ast::{Crate, Eatable},
    ir::layout::TargetDataLayout,
    irgen::IRGenerator,
    lexer::{Lexer, TokenBuffer},
    semantics::analyzer::SemanticAnalyzer,
};

const PRELUDE: &str = include_str!("../tests/prelude.c");

fn main() {
    let mut program = String::new();
    stdin().read_to_string(&mut program).unwrap();

    let lexer = Lexer::new(&program);
    let buffer = TokenBuffer::new(lexer).unwrap();
    let mut iter = buffer.iter();
    let krate = Crate::eat(&mut iter).unwrap();

    let (analyzer, semantic_result) = SemanticAnalyzer::visit(&krate);
    semantic_result.unwrap();

    let mut ir_gen = IRGenerator::new(&analyzer, TargetDataLayout::rv32());
    ir_gen.visit(&krate);
    let ir = ir_gen.print();

    print!("{}", ir);
    eprintln!("{}", PRELUDE);
}
