use crate::{
    ast::NodeId,
    ir::core::{BlockRef, ValueId},
    irgen::value::{CoreValueContainer, ValuePtrContainer},
    semantics::{item::AssociatedInfo, resolved_ty::ResolvedTyInstance},
};

#[derive(Debug, Clone, Copy)]
pub(crate) struct CoreCycleInfo {
    pub(crate) continue_bb: BlockRef,
    pub(crate) next_bb: BlockRef,
    pub(crate) value: Option<ValueId>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ExprExtra<'tmp> {
    pub(crate) scope_id: NodeId,
    pub(crate) self_id: NodeId,

    pub(crate) core_cycle_info: Option<CoreCycleInfo>,
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
