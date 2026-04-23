use crate::mir::{LoweringTarget, MachineFunction, TargetInst, lower::Lowerer};

impl<T: LoweringTarget> Lowerer<T> {
    pub(crate) fn peephole_optimize(&mut self, machine_function: &mut MachineFunction<T>) {
        for block in machine_function.blocks.iter_mut() {
            block.instructions.retain(|inst| {
                if let Some((src, dst)) = inst.as_move()
                    && src == dst {
                        return false;
                    }
                true
            });
        }
    }
}
