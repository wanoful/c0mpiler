use std::collections::HashSet;
use std::rc::Rc;

use crate::ir::{
    cfg::{CFGNode, ControlFlowGraph, DominatorTree},
    core::{BlockId, BlockRef, BlockUse, FunctionId, InstRef, ModuleCore, ValueId},
    core_inst::InstKind,
    ir_type::{Type, VoidType},
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

        let mut loops = self.collect_loops(id, &cfg, &dom_tree, &latches);

        // Try to create preheaders for loops that lack them.
        for loop_info in loops.iter_mut() {
            if loop_info.insertion_block == loop_info.header {
                if let Some(preheader) = self.try_create_preheader(id, &cfg, loop_info) {
                    loop_info.insertion_block = preheader;
                }
            }
        }

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
                for pred in cfg
                    .preds
                    .get(&CFGNode::Block(block))
                    .cloned()
                    .unwrap_or_default()
                {
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
        let idom_node = dom_tree
            .idom
            .get(&header_node)
            .copied()
            .unwrap_or(header_node);

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

    /// Create a preheader block for a loop that lacks one.
    /// Returns the new preheader block id, or None if creation fails.
    fn try_create_preheader(
        &mut self,
        func_id: FunctionId,
        cfg: &ControlFlowGraph,
        loop_info: &LoopInfo,
    ) -> Option<BlockId> {
        let header = loop_info.header;
        let header_node = CFGNode::Block(header);

        // Find predecessors of the header that are outside the loop body.
        let outside_preds: Vec<BlockId> = cfg
            .preds
            .get(&header_node)
            .map(|preds| {
                preds
                    .iter()
                    .filter_map(|p| p.as_block().copied())
                    .filter(|b| !loop_info.blocks.contains(b))
                    .collect()
            })
            .unwrap_or_default();

        if outside_preds.is_empty() {
            return None;
        }

        // Create preheader block
        let preheader_ref = self.append_block(func_id, Some("preheader".to_string()));
        let preheader = preheader_ref.block;

        // Collect phi incomings to update
        let header_ref = BlockRef {
            func: func_id,
            block: header,
        };
        let phi_updates: Vec<(InstRef, Vec<(usize, ValueId)>)> = self
            .phis_in_order(header_ref)
            .iter()
            .filter_map(|&phi| {
                let phi_data = self.inst(phi).kind.clone();
                let incomings = phi_data.as_phi()?.0.clone();
                let updates: Vec<(usize, ValueId)> = incomings
                    .iter()
                    .filter_map(|(&idx, incoming)| {
                        if outside_preds.contains(&incoming.block) {
                            Some((idx, incoming.value))
                        } else {
                            None
                        }
                    })
                    .collect();
                if updates.is_empty() {
                    None
                } else {
                    Some((phi, updates))
                }
            })
            .collect();

        // Update outside predecessors' terminators to point to preheader
        for &pred in &outside_preds {
            let pred_ref = BlockRef {
                func: func_id,
                block: pred,
            };
            if let Some(term) = self.terminator(pred_ref) {
                let mut term_kind = self.inst(term).kind.clone();
                let mut slots_to_update = Vec::new();
                term_kind.for_each_block_operand(|block_id, slot| {
                    if block_id == header {
                        slots_to_update.push(slot);
                    }
                });
                if !slots_to_update.is_empty() {
                    for slot in slots_to_update {
                        term_kind.replace_block_operand(slot, preheader);
                    }
                    // Remove old block uses of header from this terminator
                    let header_block_ref = BlockRef {
                        func: func_id,
                        block: header,
                    };
                    self.block_uses_mut(header_block_ref)
                        .retain(|u| u.user != term);
                    // Add new block uses of preheader
                    term_kind.for_each_block_operand(|block_id, slot| {
                        if block_id == preheader {
                            self.block_uses_mut(preheader_ref)
                                .push(BlockUse { user: term, slot });
                        }
                    });
                    self.inst_mut(term).kind = term_kind;
                }
            }
        }

        // Add unconditional branch from preheader to header
        let branch = self.new_inst(
            func_id,
            Rc::new(Type::Void(VoidType)),
            InstKind::Branch {
                then_block: header,
                cond: None,
            },
            None,
        );
        self.set_terminator(preheader_ref, branch);

        // Update phi nodes: replace incoming blocks from outside preds with preheader
        for (phi, updates) in phi_updates {
            // Remove old incomings and add new ones pointing to preheader
            for (idx, _value) in updates.iter().rev() {
                self.phi_remove_incoming_from(phi, *idx);
            }
            for (_, value) in updates {
                self.phi_add_incoming(phi, preheader_ref, value);
            }
        }

        Some(preheader)
    }

    fn hoist_loop_invariants(&mut self, func_id: FunctionId, loop_info: &LoopInfo) {
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

                    if self.is_loop_invariant(func_id, inst_ref, loop_info) {
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

    fn is_loop_invariant(
        &self,
        func_id: FunctionId,
        inst_ref: InstRef,
        loop_info: &LoopInfo,
    ) -> bool {
        let inst = self.inst(inst_ref);

        match &inst.kind {
            InstKind::Binary { .. }
            | InstKind::ICmp { .. }
            | InstKind::GetElementPtr { .. }
            | InstKind::Zext { .. }
            | InstKind::Sext { .. }
            | InstKind::Trunc { .. }
            | InstKind::PtrToInt { .. }
            | InstKind::Select { .. } => {}
            InstKind::Load { ptr } => {
                let parent_block = inst.parent.unwrap();
                if !loop_info.blocks.contains(&parent_block.block) {
                    return true;
                }
                if !self.is_value_invariant(*ptr, loop_info) {
                    return false;
                }
                return self.is_load_safe_to_hoist(func_id, *ptr, loop_info);
            }
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

    fn is_load_safe_to_hoist(
        &self,
        func_id: FunctionId,
        load_ptr: ValueId,
        loop_info: &LoopInfo,
    ) -> bool {
        for &block_id in loop_info.blocks.iter() {
            let block_ref = BlockRef {
                func: func_id,
                block: block_id,
            };
            for inst_ref in self.insts_in_order(block_ref) {
                let kind = self.inst(inst_ref).kind.clone();
                match kind {
                    InstKind::Store { ptr, .. } => {
                        if self.licm_ptr_may_alias(ptr, load_ptr) {
                            return false;
                        }
                    }
                    InstKind::Call { .. } => return false,
                    _ => {}
                }
            }
        }
        true
    }

    fn licm_ptr_may_alias(&self, lhs: ValueId, rhs: ValueId) -> bool {
        if lhs == rhs {
            return true;
        }

        let Some((lhs_root, lhs_path)) = self.licm_ptr_path(lhs) else {
            return true;
        };
        let Some((rhs_root, rhs_path)) = self.licm_ptr_path(rhs) else {
            return true;
        };
        if lhs_root != rhs_root {
            // Different roots: only safe if both are distinct allocas/args/globals.
            return !self.roots_known_distinct(lhs_root, rhs_root);
        }

        for (lhs_index, rhs_index) in lhs_path.iter().zip(rhs_path.iter()) {
            if lhs_index == rhs_index {
                continue;
            }
            if self.licm_const_indices_equal(*lhs_index, *rhs_index) {
                continue;
            }
            if self.licm_const_indices_known_distinct(*lhs_index, *rhs_index) {
                return false;
            }
            return true;
        }
        true
    }

    fn licm_ptr_path(&self, ptr: ValueId) -> Option<(ValueId, Vec<ValueId>)> {
        let ValueId::Inst(inst_ref) = ptr else {
            return Some((ptr, Vec::new()));
        };
        let InstKind::GetElementPtr { base, indices, .. } = &self.inst(inst_ref).kind else {
            return Some((ptr, Vec::new()));
        };

        let (root, mut path) = self.licm_ptr_path(*base)?;
        path.extend(indices.iter().copied());
        Some((root, path))
    }

    fn roots_known_distinct(&self, lhs: ValueId, rhs: ValueId) -> bool {
        // Two distinct allocas / args / globals are guaranteed not to alias.
        let is_root = |v: ValueId| {
            matches!(v, ValueId::Arg(..) | ValueId::Global(..))
                || matches!(v, ValueId::Inst(i) if matches!(self.inst(i).kind, InstKind::Alloca { .. }))
        };
        is_root(lhs) && is_root(rhs)
    }

    fn licm_const_indices_known_distinct(&self, lhs: ValueId, rhs: ValueId) -> bool {
        let (ValueId::Const(lhs), ValueId::Const(rhs)) = (lhs, rhs) else {
            return false;
        };
        use crate::ir::core_value::ConstKind;
        let ConstKind::Int(lhs) = &self.const_data(lhs).kind else {
            return false;
        };
        let ConstKind::Int(rhs) = &self.const_data(rhs).kind else {
            return false;
        };
        lhs != rhs
    }

    fn licm_const_indices_equal(&self, lhs: ValueId, rhs: ValueId) -> bool {
        let (ValueId::Const(lhs), ValueId::Const(rhs)) = (lhs, rhs) else {
            return false;
        };
        use crate::ir::core_value::ConstKind;
        let ConstKind::Int(lhs) = &self.const_data(lhs).kind else {
            return false;
        };
        let ConstKind::Int(rhs) = &self.const_data(rhs).kind else {
            return false;
        };
        lhs == rhs
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
        self.func(inst_ref.func).insts.contains_key(inst_ref.inst)
    }
}
