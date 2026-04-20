use std::collections::HashMap;

use crate::mir::{
    BlockId, LoweringTarget, MachineBlock, MachineFunction, TargetInst,
    lower::{LowerError, Lowerer},
};

impl<T: LoweringTarget> Lowerer<T> {
    fn compute_block_offsets(
        &self,
        machine_function: &MachineFunction<T>,
    ) -> HashMap<BlockId, usize> {
        let mut offsets = HashMap::new();
        let mut current_offset = 0;

        for block in machine_function.blocks.iter() {
            offsets.insert(block.id, current_offset);
            for inst in block.instructions.iter() {
                current_offset += inst.size_in_bytes();
            }
        }

        offsets
    }

    pub(crate) fn relax_branches(
        &self,
        machine_function: &mut MachineFunction<T>,
    ) -> Result<(), LowerError> {
        let range = T::branch_offset_range();

        let mut changed = true;

        while changed {
            changed = false;

            let block_offsets = self.compute_block_offsets(machine_function);
            let mut rewrite: Option<(usize, usize, BlockId)> = None;

            'outer: for block_index in 0..machine_function.blocks.len() {
                let block = &machine_function.blocks[block_index];
                let mut current_offset = block_offsets[&block.id];

                for (inst_index, inst) in block.instructions.iter().enumerate() {
                    let mut inst_clone = inst.clone();
                    let Some(target_block_id) = inst_clone.get_branch_target() else {
                        current_offset += inst.size_in_bytes();
                        continue;
                    };

                    let branch_offset =
                        block_offsets[&target_block_id] as isize - current_offset as isize;
                    if !range.contains(&branch_offset) {
                        rewrite = Some((block_index, inst_index, *target_block_id));
                        break 'outer;
                    }

                    current_offset += inst.size_in_bytes();
                }
            }

            if let Some((block_index, inst_index, old_target)) = rewrite {
                changed = true;

                let new_block_id = BlockId(machine_function.blocks.len());
                let springboard = MachineBlock {
                    id: new_block_id,
                    name: format!(".bb.springboard_{}", new_block_id.0),
                    instructions: vec![T::emit_jump(old_target)],
                };

                if let Some(target) = machine_function.blocks[block_index].instructions[inst_index]
                    .get_branch_target()
                {
                    *target = new_block_id;
                }

                machine_function.blocks.insert(block_index + 1, springboard);
            }
        }

        Ok(())
    }
}
