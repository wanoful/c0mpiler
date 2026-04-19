use std::collections::HashMap;

use crate::mir::{FrameLayout, LoweringTarget, StackSlotKind, lower::Lowerer};

impl<T: LoweringTarget> Lowerer<T> {
    pub(crate) fn compute_frame_layout(
        &self,
        machine_function: &mut crate::mir::MachineFunction<T>,
    ) {
        let ra_slot = if machine_function.frame_info.need_save_ra {
            Some(
                if let Some(ra_slot) = machine_function.frame_layout.ra_slot {
                    ra_slot
                } else {
                    machine_function.new_stack_slot(
                        T::WORD_SIZE,
                        T::WORD_SIZE,
                        StackSlotKind::LocalTemp,
                    )
                },
            )
        } else {
            None
        };

        let mut used_callee_saved = machine_function
            .frame_info
            .used_callee_saved
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        used_callee_saved.sort();
        let callee_saved_slots = used_callee_saved
            .into_iter()
            .map(|phy| {
                if let Some(slot) = machine_function.frame_layout.callee_saved_slots.get(&phy) {
                    (phy, *slot)
                } else {
                    let slot = machine_function.new_stack_slot(
                        T::WORD_SIZE,
                        T::WORD_SIZE,
                        StackSlotKind::CalleeSaved,
                    );
                    (phy, slot)
                }
            })
            .collect::<HashMap<_, _>>();

        let frame_info = &machine_function.frame_info;

        let outgoing_arg_size = frame_info.max_outgoing_arg_size;

        let mut current_offset = outgoing_arg_size.next_multiple_of(T::WORD_SIZE);
        let mut slot_offsets = HashMap::new();
        for slot in frame_info.stack_slots.iter() {
            current_offset = current_offset.next_multiple_of(slot.align);
            slot_offsets.insert(slot.id, current_offset as isize);
            current_offset += slot.size;
        }

        let frame_align = frame_info.max_align.max(T::WORD_SIZE);
        let frame_size = current_offset.next_multiple_of(frame_align);

        let layout = FrameLayout {
            frame_size,
            slot_offsets,
            outgoing_arg_offset: 0,
            incoming_arg_offset: frame_size as isize,
            callee_saved_slots,
            ra_slot,
        };

        machine_function.frame_layout = layout;
    }
}
