pub mod ast;
pub mod ir;
pub mod irgen;
pub mod lexer;
pub mod mir;
pub mod semantics;
pub mod tokens;
pub mod utils;

#[macro_export]
macro_rules! impossible {
    () => {
        panic!("Impossible!")
    };
}
