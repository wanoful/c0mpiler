use enum_as_inner::EnumAsInner;

use crate::ir::core::{ConstId, FunctionId};

#[derive(Debug)]
pub enum ConstKind {
    Int(i64),
    Array(Vec<ConstId>),
    Struct(Vec<ConstId>),
    String(String),
    Null,
}

#[derive(Debug, EnumAsInner)]
pub enum GlobalKind {
    Function(FunctionId),
    GlobalVariable {
        is_constant: bool,
        initializer: Option<ConstId>,
    },
}
