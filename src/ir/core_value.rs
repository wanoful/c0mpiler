use crate::ir::core::{ConstId, FunctionId};

pub enum ConstKind {
    Int(i64),
    Array(Vec<ConstId>),
    Struct(Vec<ConstId>),
    String(String),
}

pub enum GlobalKind {
    Function(FunctionId),
    GlobalVariable {
        is_constant: bool,
        initializer: Option<ConstId>,
    },
}
