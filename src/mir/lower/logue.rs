use crate::mir::{LoweringTarget, MachineFunction, Register, TargetInst, lower::Lowerer};

impl<T: LoweringTarget> Lowerer<T> {
    pub(crate) fn insert_logue(&self, machine_function: &mut MachineFunction<T>) {
        let mut prologue_insts = Vec::new();
        let mut epilogue_inst_fns: Vec<Box<dyn Fn() -> Vec<T::MachineInst>>> = Vec::new();
        let rt = Register::Physical(T::spill_scratch_regs()[0]);

        prologue_insts.extend(T::emit_adjust_sp(
            -(machine_function.frame_layout.frame_size as isize),
        ));
        let frame_size = machine_function.frame_layout.frame_size;
        epilogue_inst_fns.push(Box::new(move || T::emit_adjust_sp(frame_size as isize)));

        if let Some(ra_slot) = machine_function.frame_layout.ra_slot {
            prologue_insts.push(T::emit_store_stack_slot(
                Register::Physical(T::ra_reg()),
                ra_slot,
                rt,
            ));
            epilogue_inst_fns.push(Box::new(move || {
                vec![T::emit_load_stack_slot(
                    Register::Physical(T::ra_reg()),
                    ra_slot,
                )]
            }));
        }

        let mut callee_saved = machine_function
            .frame_info
            .used_callee_saved
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        callee_saved.sort();
        for callee_saved in callee_saved {
            let slot = machine_function.frame_layout.callee_saved_slots[&callee_saved];
            prologue_insts.push(T::emit_store_stack_slot(
                Register::Physical(callee_saved),
                slot,
                rt,
            ));
            epilogue_inst_fns.push(Box::new(move || {
                vec![T::emit_load_stack_slot(
                    Register::Physical(callee_saved),
                    slot,
                )]
            }));
        }

        epilogue_inst_fns.reverse();
        let epilogue_insts = epilogue_inst_fns
            .into_iter()
            .flat_map(|f| f())
            .collect::<Vec<_>>();

        machine_function
            .get_block_mut(machine_function.entry)
            .unwrap()
            .instructions
            .splice(0..0, prologue_insts);

        for block in machine_function.blocks.iter_mut() {
            let mut new_insts = Vec::new();
            for inst in block.instructions.iter() {
                if inst.is_ret() {
                    new_insts.extend(epilogue_insts.clone());
                }
                new_insts.push(inst.clone());
            }
            block.instructions = new_insts;
        }
    }
}
