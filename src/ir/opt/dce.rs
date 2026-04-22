use std::collections::HashSet;

use crate::ir::{
    core::{FunctionId, InstData, InstId, InstRef, ModuleCore, ValueId},
    core_inst::InstKind,
};

impl ModuleCore {
    pub fn opt_dead_code_elimination(&mut self) {
        for id in self.functions_in_order() {
            self.func_dead_code_elimination(id);
        }
    }

    fn inst_is_always_live(&self, inst: &InstData) -> bool {
        match &inst.kind {
            InstKind::Call { .. }
            | InstKind::Branch { .. }
            | InstKind::Ret { .. }
            | InstKind::Store { .. }
            | InstKind::Unreachable => true,
            _ => false,
        }
    }

    fn func_dead_code_elimination(&mut self, id: FunctionId) {
        let function = self.func(id);
        if function.is_declare {
            return;
        }

        let mut work_list = function
            .insts
            .iter()
            .filter_map(|(inst_id, data)| {
                if self.inst_is_always_live(data) {
                    Some(inst_id)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let mut live_insts: HashSet<InstId> = HashSet::from_iter(work_list.clone());

        while let Some(live_id) = work_list.pop() {
            let live_inst = &function.insts[live_id];
            live_inst.kind.for_each_value_operand(|v, _| {
                if let ValueId::Inst(InstRef { inst, .. }) = v {
                    if !live_insts.contains(&inst) {
                        live_insts.insert(inst);
                        work_list.push(inst);
                    }
                }
            });
        }

        let to_be_removed = function
            .insts
            .iter()
            .filter_map(|(inst_id, _)| {
                if !live_insts.contains(&inst_id) {
                    Some(inst_id)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        for inst_id in to_be_removed {
            self.erase_inst_from_parent_forcely(InstRef {
                func: id,
                inst: inst_id,
            });
        }
    }
}
