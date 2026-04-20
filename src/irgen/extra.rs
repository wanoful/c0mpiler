use crate::{
    ast::NodeId,
    ir::core::ValueId,
    ir::ir_value::{ArgumentPtr, BasicBlockPtr, ValuePtr},
    irgen::value::{CoreValueContainer, ValuePtrContainer},
    semantics::{item::AssociatedInfo, resolved_ty::ResolvedTyInstance},
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct CycleInfo<'tmp> {
    pub(crate) continue_bb: &'tmp BasicBlockPtr,
    pub(crate) next_bb: &'tmp BasicBlockPtr,
    pub(crate) value: Option<&'tmp ValuePtr>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ExprExtra<'tmp> {
    pub(crate) scope_id: NodeId,
    pub(crate) self_id: NodeId,

    pub(crate) cycle_info: Option<CycleInfo<'tmp>>,
    pub(crate) ret_ptr: Option<&'tmp ArgumentPtr>,
    pub(crate) core_ret_ptr: Option<ValueId>,

    pub(crate) self_ty: Option<&'tmp ResolvedTyInstance>,
}

#[derive(Debug)]
pub(crate) struct ItemExtra {
    pub(crate) scope_id: NodeId,
    pub(crate) self_id: NodeId,

    pub(crate) associated_info: Option<AssociatedInfo>,
}

pub(crate) struct PatExtra {
    pub(crate) value: ValuePtrContainer,
    pub(crate) core_value: Option<CoreValueContainer>,
    pub(crate) self_id: NodeId,
    pub(crate) is_temp_value: bool,
}
