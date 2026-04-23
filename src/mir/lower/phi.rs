use crate::{
    ir::{
        core::{BlockRef, ModuleCore, ValueId},
        core_inst::InstKind,
    },
    mir::{
        BlockId, LoweringTarget, MachineFunction, VRegId,
        lower::{FunctionLoweringState, LowerError, Lowerer},
    },
};

#[derive(Debug)]
pub(super) struct PhiIncoming {
    pred: BlockId,
    value: ValueId,
}

#[derive(Debug)]
pub(super) struct PhiInfo {
    dst: VRegId,
    incomings: Vec<PhiIncoming>,
}

impl PhiInfo {
    pub(super) fn get_dst(&self) -> VRegId {
        self.dst
    }

    pub(super) fn filter_pred(&self, pred: BlockId) -> Option<ValueId> {
        self.incomings.iter().find_map(|incoming| {
            if incoming.pred == pred {
                Some(incoming.value)
            } else {
                None
            }
        })
    }
}

impl<T: LoweringTarget> Lowerer<T> {
    pub(super) fn collect_phis(
        &mut self,
        module: &ModuleCore,
        machine_function: &mut MachineFunction<T>,
        state: &mut FunctionLoweringState,
    ) -> Result<(), LowerError> {
        for block in state.block_order.clone() {
            let block_id = state.block_id(&block).unwrap();
            let phi_infos = module
                .phis_in_order(block)
                .into_iter()
                .map(|phi| {
                    let inst = module.inst(phi);
                    let incomings = match &inst.kind {
                        InstKind::Phi { incomings, .. } => incomings
                            .values()
                            .map(|incoming| {
                                let pred_block = BlockRef {
                                    func: block.func,
                                    block: incoming.block,
                                };
                                let pred = state.block_id(&pred_block).unwrap();
                                PhiIncoming {
                                    pred,
                                    value: incoming.value,
                                }
                            })
                            .collect(),
                        _ => unreachable!("phis_in_order must only return phi instructions"),
                    };
                    let dst = machine_function.new_vreg();
                    state.record_vreg(ValueId::Inst(phi), dst);
                    PhiInfo { dst, incomings }
                })
                .collect();
            state.phi_infos.insert(block_id, phi_infos);
        }
        Ok(())
    }
}
