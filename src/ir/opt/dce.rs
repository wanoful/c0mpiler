use std::{collections::HashSet, rc::Rc};

use crate::ir::{
    cfg::{CFGNode, ControlFlowGraph, DFSResult, DominatorTree},
    core::{FunctionId, InstData, InstId, InstRef, ModuleCore, ValueId},
    core_inst::{InstKind, PhiIncoming},
    ir_type::{Type, VoidType},
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
                if let ValueId::Inst(InstRef { inst, .. }) = v
                    && !live_insts.contains(&inst) {
                        live_insts.insert(inst);
                        work_list.push(inst);
                    }
            });
        }

        self.remove_dead_instructions(id, live_insts, None);
    }

    fn remove_dead_instructions(
        &mut self,
        id: FunctionId,
        live_insts: HashSet<InstId>,
        reverse_dom_tree: Option<&DominatorTree>,
    ) {
        let function = self.func(id);
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
            let inst = &self.func(id).insts[inst_id];
            if inst.kind.is_terminator() {
                if matches!(inst.kind, InstKind::Unreachable) {
                    continue;
                }
                let inst_ref = InstRef {
                    func: id,
                    inst: inst_id,
                };
                let parent = inst.parent.unwrap();
                let target = reverse_dom_tree.unwrap().idom[&parent.block.into()]
                    .into_block()
                    .unwrap();
                let new_inst = self.new_inst(
                    id,
                    Rc::new(Type::Void(VoidType)),
                    InstKind::Branch {
                        then_block: target,
                        cond: None,
                    },
                    None,
                );
                self.overwrite_inst(inst_ref, new_inst);
            } else {
                self.erase_inst_from_parent_forcely(InstRef {
                    func: id,
                    inst: inst_id,
                });
            }
        }
    }

    pub fn opt_aggressive_dead_code_elimination(&mut self) {
        for id in self.functions_in_order() {
            self.func_aggressive_dead_code_elimination(id);
        }
    }

    fn inst_is_always_live_aggressively(&self, inst: &InstData) -> bool {
        // TODO: 根据对应函数是否有副作用来判断调用指令是否总是活跃的。
        match &inst.kind {
            InstKind::Call { .. }
            | InstKind::Ret { .. }
            | InstKind::Store { .. }
            | InstKind::Unreachable => true,
            _ => false,
        }
    }

    pub(crate) fn func_aggressive_dead_code_elimination(&mut self, id: FunctionId) {
        let function = self.func(id);
        if function.is_declare {
            return;
        }

        let ends = function
            .blocks
            .iter()
            .filter_map(|(id, data)| {
                if let Some(inst) = data.terminator
                    && matches!(
                        function.insts[inst].kind,
                        InstKind::Ret { .. } | InstKind::Unreachable
                    )
                {
                    Some(id)
                } else {
                    None
                }
            })
            .collect();

        let mut cfg = self.build_cfg(id);
        let DFSResult { order, .. } = cfg.build_dfn();
        let front_reachable: HashSet<CFGNode> = HashSet::from_iter(order);

        let dom_tree = cfg.build_dom_tree();

        let latch_insts = build_loop_latches(&cfg, &dom_tree)
            .into_iter()
            .map(|node| {
                let block_id = node.into_block().unwrap();
                function.blocks[block_id].terminator.unwrap()
            })
            .collect::<HashSet<_>>();

        cfg.reverse(ends);

        let mut reverse_cfg = cfg;
        let DFSResult {
            order: reverse_order,
            ..
        } = reverse_cfg.build_dfn();

        let back_reachable: HashSet<CFGNode> = HashSet::from_iter(reverse_order);
        let dead_ends = front_reachable
            .difference(&back_reachable)
            .collect::<HashSet<_>>();

        for &node in dead_ends.iter() {
            reverse_cfg
                .succs
                .entry(reverse_cfg.entry)
                .or_default()
                .insert(*node);
        }

        let reverse_dom_tree = reverse_cfg.build_dom_tree();
        let reverse_dom_frontiers = reverse_cfg.build_dom_frontier(&reverse_dom_tree);

        let mut live_insts = function
            .insts
            .iter()
            .filter_map(|(id, data)| {
                if let Some(parent) = data.parent
                    && front_reachable.contains(&CFGNode::Block(parent.block))
                    && self.inst_is_always_live_aggressively(data)
                {
                    Some(id)
                } else {
                    None
                }
            })
            .collect::<HashSet<_>>();

        live_insts.extend(latch_insts);
        let mut work_list = live_insts.iter().copied().collect::<Vec<_>>();

        while let Some(inst_id) = work_list.pop() {
            let inst = &function.insts[inst_id];
            inst.kind.for_each_value_operand(|v, _| {
                if let ValueId::Inst(InstRef { inst, .. }) = v
                    && !live_insts.contains(&inst) {
                        live_insts.insert(inst);
                        work_list.push(inst);
                    }
            });

            // Phi 的控制依赖也需要保活
            if let InstKind::Phi { incomings, .. } = &inst.kind {
                for PhiIncoming { block, .. } in incomings.values() {
                    let block = &function.blocks[*block];
                    if let Some(term) = block.terminator
                        && !live_insts.contains(&term) {
                            live_insts.insert(term);
                            work_list.push(term);
                        }
                }
            }

            let Some(frontier) =
                reverse_dom_frontiers.get(&CFGNode::Block(inst.parent.unwrap().block))
            else {
                continue;
            };
            for &f in frontier.iter() {
                let block_id = f.into_block().unwrap();
                let block = &function.blocks[block_id];
                if let Some(term) = block.terminator
                    && !live_insts.contains(&term) {
                        live_insts.insert(term);
                        work_list.push(term);
                    }
            }
        }

        self.remove_dead_instructions(id, live_insts, Some(&reverse_dom_tree));
    }
}

fn build_loop_latches(cfg: &ControlFlowGraph, dom_tree: &DominatorTree) -> HashSet<CFGNode> {
    let mut latch_nodes = HashSet::new();

    // println!("Building loop latches for CFG with {:#?} nodes", cfg);

    for (&node, succs) in &cfg.succs {
        for &succ in succs {
            if dom_tree.dominates(succ, node) {
                assert!(
                    matches!(succ, CFGNode::Block(_)) && matches!(node, CFGNode::Block(_)),
                    "Back edge must be between blocks, bu get {:?} -> {:?}",
                    node,
                    succ
                );
                latch_nodes.insert(node);
            }
        }
    }

    latch_nodes
}
