use std::{collections::HashSet, fmt::Display};

use enum_as_inner::EnumAsInner;

use crate::{ast::Symbol, semantics::analyzer::SemanticAnalyzer};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FullName(pub Vec<Symbol>);

impl FullName {
    pub(crate) fn concat(mut self, s: Symbol) -> Self {
        self.0.push(s);
        self
    }

    pub(crate) fn append(mut self, mut right: FullName) -> Self {
        self.0.append(&mut right.0);
        self
    }
}

impl Display for FullName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", {
            let s: String = self
                .0
                .iter()
                .map(|x| x.0.clone())
                .collect::<Vec<_>>()
                .join(".");
        if s.starts_with(|x: char| x.is_ascii_digit()) {
            "_".to_owned() + &s
        } else {
            s
        }
        })
    }
}

#[test]
fn full_name_test() {
    let name = FullName(vec![
        Symbol("hello".to_string()),
        Symbol("world".to_string()),
        Symbol("!".to_string()),
    ]);

    println!("{name}");
}

#[derive(Debug, EnumAsInner, Clone, Copy, PartialEq, Eq)]
pub enum AnalyzeStage {
    SymbolCollect,
    Definition,
    Body,
}

pub const STAGES: [AnalyzeStage; 3] = [
    AnalyzeStage::SymbolCollect,
    AnalyzeStage::Definition,
    AnalyzeStage::Body,
];

pub(crate) fn is_all_different<T: Eq + std::hash::Hash>(vec: &[T]) -> bool {
    let mut set = HashSet::new();
    for x in vec {
        if !set.insert(x) {
            return false;
        }
    }
    true
}

impl<'ast> SemanticAnalyzer<'ast> {
    pub fn is_body_stage(&self) -> bool {
        self.stage.is_body()
    }
}
