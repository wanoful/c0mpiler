use crate::{
    ir::ir_value::ValuePtr,
    mir::{
        BlockId, LoweringTarget, MachineFunction, VRegId,
        lower::{FunctionLoweringState, LowerError, Lowerer},
    },
};

#[derive(Debug)]
pub(super) struct PhiIncoming {
    pred: BlockId,
    value: ValuePtr,
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

    pub(super) fn filter_pred(&self, pred: BlockId) -> Option<&ValuePtr> {
        self.incomings.iter().find_map(|incoming| {
            if incoming.pred == pred {
                Some(&incoming.value)
            } else {
                None
            }
        })
    }
}

impl<T: LoweringTarget> Lowerer<T> {
    pub(super) fn collect_phis(
        &mut self,
        machine_function: &mut MachineFunction<T>,
        state: &mut FunctionLoweringState,
    ) -> Result<(), LowerError> {
        for block in state.block_order.clone().iter() {
            let block_id = state.block_id(block).unwrap();
            let phi_infos = block
                .as_basic_block()
                .instructions
                .borrow()
                .iter()
                .map_while(|ptr| {
                    let inst = ptr.as_instruction();
                    if !matches!(inst.kind, crate::ir::ir_value::InstructionKind::Phi) {
                        return None;
                    }
                    let incomings = inst
                        .operands
                        .chunks(2)
                        .map(|chunk| {
                            let value = &chunk[0];
                            let succ_block = &chunk[1];
                            let succ_block_id = state.block_id(succ_block).unwrap();
                            PhiIncoming {
                                pred: succ_block_id,
                                value: value.clone(),
                            }
                        })
                        .collect();
                    let dst = machine_function.new_vreg();
                    state.record_vreg(ptr, dst);
                    Some(PhiInfo { dst, incomings })
                })
                .collect();
            state.phi_infos.insert(block_id, phi_infos);
        }
        Ok(())
    }
}
