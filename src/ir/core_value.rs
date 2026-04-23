use enum_as_inner::EnumAsInner;

use crate::ir::{
    core::{ConstId, FunctionId},
    core_int::CoreInt,
};

#[derive(Debug)]
pub enum ConstKind {
    Int(CoreInt),
    Array(Vec<ConstId>),
    Struct(Vec<ConstId>),
    String(String),
    Null,
    Undef,
}

#[derive(Debug, EnumAsInner)]
pub enum GlobalKind {
    Function(FunctionId),
    GlobalVariable {
        is_constant: bool,
        initializer: Option<ConstId>,
    },
}
