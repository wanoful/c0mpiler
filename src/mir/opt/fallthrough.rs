use crate::mir::{LoweringTarget, MachineFunction, lower::Lowerer};

impl<T: LoweringTarget> Lowerer<T> {
    pub(crate) fn optimize_fallthroughs(&self, machine_function: &mut MachineFunction<T>) {
        let order = machine_function
            .blocks
            .iter()
            .map(|b| b.id)
            .collect::<Vec<_>>();
        for (block, next_id) in machine_function
            .blocks
            .iter_mut()
            .zip(order.into_iter().skip(1))
        {
            if let Some(last_inst) = block.instructions.last() {
                if T::is_jump_to(last_inst, next_id) {
                    block.instructions.pop();
                }
            }
        }
    }
}
