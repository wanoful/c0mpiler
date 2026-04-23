use std::collections::HashSet;

use crate::ir::{
    cfg::{CFGNode, DFSResult},
    core::{BlockRef, FunctionId, InstRef, ModuleCore, ValueId},
    core_inst::PhiIncoming,
};

impl ModuleCore {
    pub fn opt_cfg_simplify(&mut self) {
        for id in self.functions_in_order() {
            self.func_cfg_simplify(id);
        }
    }

    fn func_cfg_simplify(&mut self, id: FunctionId) {
        let function = self.func(id);
        if function.is_declare {
            return;
        }

        self.dead_block_elimination(id);
        self.block_merge(id);
    }

    fn useless_phi_elimination(&mut self, id: FunctionId) {
        let function = self.func(id);
        let phis = function
            .blocks
            .values()
            .flat_map(|block| block.phis.iter())
            .cloned()
            .collect::<Vec<_>>();

        let cfg = self.build_cfg(id);

        phis.into_iter().for_each(|phi| {
            let phi_ref = InstRef {
                func: id,
                inst: phi,
            };
            let inst = self.inst(phi_ref);
            let parent_block = inst.parent.unwrap();
            let remove_labels = inst
                .kind
                .as_phi()
                .unwrap()
                .0
                .iter()
                .filter_map(|(idx, PhiIncoming { block, .. })| {
                    if !cfg.preds[&CFGNode::Block(parent_block.block)]
                        .contains(&CFGNode::Block(*block))
                    {
                        Some(*idx)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            for remove_label in remove_labels {
                self.phi_remove_incoming_from(phi_ref, remove_label);
            }

            let inst = self.inst(phi_ref);
            if inst.kind.as_phi().unwrap().0.len() == 1 {
                let v = inst.kind.as_phi().unwrap().0.values().next().unwrap().value;
                self.replace_all_uses_with(ValueId::Inst(phi_ref), v);
                self.erase_inst_from_parent(phi_ref);
            }
        });
    }

    fn dead_block_elimination(&mut self, id: FunctionId) {
        let cfg = self.build_cfg(id);
        let DFSResult {
            order: dfs_order, ..
        } = cfg.build_dfn();
        let reachable_blocks = dfs_order
            .into_iter()
            .filter_map(|node| node.as_block().cloned())
            .collect::<HashSet<_>>();

        let dead_blocks = self
            .func(id)
            .block_order
            .iter()
            .filter_map(|block_id| {
                (!reachable_blocks.contains(block_id)).then_some(crate::ir::core::BlockRef {
                    func: id,
                    block: *block_id,
                })
            })
            .collect::<Vec<_>>();

        self.erase_blocks_from_parent(dead_blocks);

        self.useless_phi_elimination(id);
    }

    fn block_merge(&mut self, id: FunctionId) {
        let mut visited = HashSet::new();
        let mut work_list = self.func(id).block_order.clone();

        let cfg = self.build_cfg(id);
        let mergerable_block = work_list
            .iter()
            .filter(|&&x| cfg.preds[&CFGNode::Block(x)].len() == 1)
            .cloned()
            .collect::<HashSet<_>>();

        let mut removed = HashSet::new();

        while let Some(block_id) = work_list.pop() {
            if !visited.insert(block_id) {
                continue;
            }
            let block_ref = BlockRef {
                func: id,
                block: block_id,
            };

            let Some(term) = self.terminator(block_ref) else {
                continue;
            };

            let succ = self.successors(term);
            if succ.len() != 1 {
                continue;
            }

            let succ_ref = *succ.first().unwrap();

            if !mergerable_block.contains(&succ_ref.block) {
                continue;
            }

            let phis = self.phis_in_order(succ_ref);
            assert!(
                phis.is_empty(),
                "Cannot merge block {:?} into {:?} because the successor has phi nodes",
                block_ref,
                succ_ref
            );
            let insts = self.insts_in_order(succ_ref);
            insts.iter().for_each(|inst| {
                self.detach_inst(*inst);
                self.append_inst(block_ref, *inst);
            });

            if let Some(term) = self.terminator(succ_ref) {
                let old_term = self.terminator(block_ref).unwrap();
                self.erase_inst_from_parent(old_term);
                self.detach_inst(term);
                self.set_terminator(block_ref, term);
            }

            self.replace_all_block_uses_with(succ_ref, block_ref);
            removed.insert(succ_ref);
            visited.remove(&block_id);
            work_list.push(block_id);
        }
        self.erase_blocks_from_parent(removed.into_iter().collect());
        self.useless_phi_elimination(id);
    }
}
