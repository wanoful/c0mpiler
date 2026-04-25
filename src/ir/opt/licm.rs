use std::collections::HashSet;

use crate::ir::{
    cfg::{CFGNode, ControlFlowGraph, DominatorTree},
    core::{BlockId, BlockRef, FunctionId, InstRef, ModuleCore, ValueId},
    core_inst::InstKind,
};

struct LoopInfo {
    #[allow(dead_code)]
    header: BlockId,
    insertion_block: BlockId,
    blocks: HashSet<BlockId>,
}

impl ModuleCore {
    pub fn opt_licm(&mut self) {
        for id in self.functions_in_order() {
            self.func_licm(id);
        }
    }

    fn func_licm(&mut self, id: FunctionId) {
        let function = self.func(id);
        if function.is_declare {
            return;
        }

        let cfg = self.build_cfg(id);
        let dom_tree = cfg.build_dom_tree();
        let latches = self.build_licm_loop_latches(&cfg, &dom_tree);

        if latches.is_empty() {
            return;
        }

        let loops = self.collect_loops(id, &cfg, &dom_tree, &latches);

        for loop_info in loops.iter() {
            self.hoist_loop_invariants(id, loop_info);
        }
    }

    fn build_licm_loop_latches(
        &self,
        cfg: &ControlFlowGraph,
        dom_tree: &DominatorTree,
    ) -> HashSet<(BlockId, BlockId)> {
        let mut latches = HashSet::new();
        for (&node, succs) in &cfg.succs {
            for &succ in succs {
                if dom_tree.dominates(succ, node) {
                    if let (CFGNode::Block(latch), CFGNode::Block(header)) = (node, succ) {
                        latches.insert((latch, header));
                    }
                }
            }
        }
        latches
    }

    fn collect_loops(
        &self,
        func: FunctionId,
        cfg: &ControlFlowGraph,
        dom_tree: &DominatorTree,
        latches: &HashSet<(BlockId, BlockId)>,
    ) -> Vec<LoopInfo> {
        let mut loops: Vec<LoopInfo> = Vec::new();

        for &(latch, header) in latches {
            let mut blocks: HashSet<BlockId> = HashSet::new();
            blocks.insert(latch);
            blocks.insert(header);

            let mut work_list = vec![latch];
            while let Some(block) = work_list.pop() {
                for pred in cfg.preds.get(&CFGNode::Block(block)).cloned().unwrap_or_default() {
                    if let CFGNode::Block(pred_id) = pred {
                        if pred_id != header && blocks.insert(pred_id) {
                            work_list.push(pred_id);
                        }
                    }
                }
            }

            let insertion_block = self.find_insertion_block(func, header, &blocks, dom_tree);

            loops.push(LoopInfo {
                header,
                insertion_block,
                blocks,
            });
        }

        loops
    }

    fn find_insertion_block(
        &self,
        _func: FunctionId,
        header: BlockId,
        loop_blocks: &HashSet<BlockId>,
        dom_tree: &DominatorTree,
    ) -> BlockId {
        let header_node = CFGNode::Block(header);
        let idom_node = dom_tree.idom.get(&header_node).copied().unwrap_or(header_node);

        match idom_node {
            CFGNode::Block(idom_block) => {
                if idom_block != header && !loop_blocks.contains(&idom_block) {
                    return idom_block;
                }
            }
            CFGNode::Fake => {}
        }

        header
    }

    fn hoist_loop_invariants(
        &mut self,
        func_id: FunctionId,
        loop_info: &LoopInfo,
    ) {
        if loop_info.insertion_block == loop_info.header {
            return;
        }

        let mut changed = true;
        while changed {
            changed = false;

            let block_ids: Vec<BlockId> = loop_info.blocks.iter().copied().collect();
            for &block_id in &block_ids {
                let block_ref = BlockRef {
                    func: func_id,
                    block: block_id,
                };

                let insts: Vec<InstRef> = self.insts_in_order(block_ref);
                for inst_ref in insts {
                    if !self.inst_exists_licm(inst_ref) {
                        continue;
                    }

                    if self.is_loop_invariant(inst_ref, loop_info) {
                        let inst = self.inst(inst_ref);
                        if inst.kind.is_phi() || inst.kind.is_terminator() {
                            continue;
                        }

                        let insert_ref = BlockRef {
                            func: func_id,
                            block: loop_info.insertion_block,
                        };

                        self.detach_inst(inst_ref);
                        self.append_inst(insert_ref, inst_ref);
                        changed = true;
                    }
                }
            }
        }
    }

    fn is_loop_invariant(&self, inst_ref: InstRef, loop_info: &LoopInfo) -> bool {
        let inst = self.inst(inst_ref);

        match &inst.kind {
            InstKind::Binary { .. } | InstKind::ICmp { .. } => {}
            _ => return false,
        }

        let parent_block = inst.parent.unwrap();
        if !loop_info.blocks.contains(&parent_block.block) {
            return true;
        }

        let mut all_invariant = true;
        inst.kind.for_each_value_operand(|value, _| {
            if !self.is_value_invariant(value, loop_info) {
                all_invariant = false;
            }
        });
        all_invariant
    }

    fn is_value_invariant(&self, value: ValueId, loop_info: &LoopInfo) -> bool {
        match value {
            ValueId::Const(..) => true,
            ValueId::Arg(..) => true,
            ValueId::Global(..) => true,
            ValueId::Inst(inst_ref) => {
                let inst = self.inst(inst_ref);
                if let Some(parent) = inst.parent {
                    !loop_info.blocks.contains(&parent.block)
                } else {
                    false
                }
            }
        }
    }

    fn inst_exists_licm(&self, inst_ref: InstRef) -> bool {
        self.func(inst_ref.func)
            .insts
            .contains_key(inst_ref.inst)
    }
}
