use std::collections::{HashMap, HashSet};

use crate::ir::{
    cfg::{CFGNode, DominatorTree},
    core::{BlockId, BlockRef, FunctionId, InstId, InstRef, ModuleCore, Use, ValueId},
    core_inst::{InstKind, OperandSlot},
};

impl ModuleCore {
    pub fn opt_pass_mem2reg(&mut self) {
        for id in self.functions_in_order() {
            self.func_mem2reg(id);
        }
    }

    fn func_mem2reg(&mut self, id: FunctionId) {
        let function = self.func(id);
        if function.is_declare {
            return;
        }

        let cfg = self.build_cfg(id);
        let dom_tree = cfg.build_dom_tree();
        let dom_frontiers = cfg.build_dom_frontier(&dom_tree);

        let allocas = function.blocks[function.entry]
            .insts
            .iter()
            .filter_map(|&inst_id| {
                let inst = &function.insts[inst_id];
                if let InstKind::Alloca { ty } = &inst.kind {
                    let mut defs = Vec::new();
                    for Use { user, slot } in inst.uses.iter() {
                        match slot {
                            OperandSlot::StorePtr => defs.push(*user),
                            OperandSlot::LoadPtr => {}
                            _ => return None,
                        }
                    }
                    Some((inst_id, defs, ty.clone()))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        let mut phi_mappings = HashMap::new();

        for (alloca, defs, ty) in allocas.iter() {
            let mut work_list: Vec<CFGNode> = defs
                .iter()
                .map(|i| self.func(id).insts[i.inst].parent.unwrap().block.into())
                .collect();
            let mut phi_inserted: HashSet<CFGNode> = HashSet::new();
            let mut worked: HashSet<CFGNode> = HashSet::from_iter(work_list.clone());

            while let Some(block_id) = work_list.pop() {
                let Some(frontier) = dom_frontiers.get(&block_id) else {
                    continue;
                };
                for &f in frontier.iter() {
                    if phi_inserted.insert(f) {
                        let phi_inst = self.new_inst(
                            id,
                            ty.clone(),
                            InstKind::Phi {
                                incomings: Vec::new(),
                            },
                            None,
                        );
                        phi_mappings.insert(phi_inst.inst, *alloca);
                        self.append_phi(
                            BlockRef {
                                func: id,
                                block: f.into_block().unwrap(),
                            },
                            phi_inst,
                        );

                        if worked.insert(f) {
                            work_list.push(f);
                        }
                    }
                }
            }
        }

        let mut stacks = allocas
            .iter()
            .map(|(alloca, _, _)| (*alloca, Vec::new()))
            .collect::<HashMap<_, _>>();
        let current_block = BlockRef {
            func: id,
            block: self.func(id).entry,
        };
        let mut active_path = Vec::new();
        self.mem2reg_rename_stage(
            current_block,
            &mut stacks,
            &dom_tree,
            &phi_mappings,
            &mut active_path,
        );

        for (alloca, _, _) in allocas.iter() {
            let inst = InstRef {
                func: id,
                inst: *alloca,
            };
            assert!(
                self.value_uses(ValueId::Inst(inst)).is_empty(),
                "alloca should have no uses after mem2reg, but found some: {:?}",
                self.value_uses(ValueId::Inst(inst))
            );
            self.erase_inst_from_parent(inst);
        }
    }

    fn mem2reg_rename_stage(
        &mut self,
        current_block: BlockRef,
        stacks: &mut HashMap<InstId, Vec<ValueId>>,
        dom_tree: &DominatorTree,
        phi_mappings: &HashMap<InstId, InstId>,
        active_path: &mut Vec<BlockId>,
    ) {
        if active_path.contains(&current_block.block) {
            eprintln!(
                "[mem2reg] dominator-tree cycle detected, path={:?}, current={:?}",
                active_path, current_block.block
            );
            return;
        }
        active_path.push(current_block.block);

        let top_or_undef = |core: &mut ModuleCore,
                            alloca_inst: InstId,
                            stacks: &mut HashMap<InstId, Vec<ValueId>>|
         -> ValueId {
            stacks[&alloca_inst].last().cloned().unwrap_or_else(|| {
                let ty = match &core
                    .inst(InstRef {
                        func: current_block.func,
                        inst: alloca_inst,
                    })
                    .kind
                {
                    InstKind::Alloca { ty } => ty.clone(),
                    _ => panic!(),
                };
                ValueId::Const(core.add_undef_const(ty))
            })
        };

        let mut pushed_count: HashMap<InstId, usize> = HashMap::new();

        for phi_id in self.phis_in_order(current_block) {
            let Some(alloca_inst) = phi_mappings.get(&phi_id.inst) else {
                continue;
            };

            stacks
                .get_mut(alloca_inst)
                .unwrap()
                .push(ValueId::Inst(phi_id));
            *pushed_count.entry(*alloca_inst).or_insert(0) += 1;
        }

        let mut to_be_removed = Vec::new();

        for inst_id in self.insts_in_order(current_block) {
            match self.inst(inst_id).kind {
                InstKind::Load {
                    ptr: ValueId::Inst(ptr),
                } => {
                    if stacks.contains_key(&ptr.inst) {
                        let value = top_or_undef(self, ptr.inst, stacks);
                        self.replace_all_uses_with(ValueId::Inst(inst_id), value);
                        to_be_removed.push(inst_id);
                    }
                }
                InstKind::Store {
                    value,
                    ptr: ValueId::Inst(ptr),
                } => {
                    if let Some(stack) = stacks.get_mut(&ptr.inst) {
                        stack.push(value);
                        *pushed_count.entry(ptr.inst).or_insert(0) += 1;
                        to_be_removed.push(inst_id);
                    }
                }
                _ => {}
            }
        }

        for inst_id in to_be_removed {
            self.erase_inst_from_parent(inst_id);
        }

        for succ in self.block_successors(current_block) {
            for phi_id in self.phis_in_order(succ) {
                let Some(alloca_inst) = phi_mappings.get(&phi_id.inst) else {
                    continue;
                };
                let value = top_or_undef(self, *alloca_inst, stacks);
                self.phi_add_incoming(phi_id, current_block, value);
            }
        }

        for &child in dom_tree.children[&current_block.block.into()].iter() {
            self.mem2reg_rename_stage(
                BlockRef {
                    func: current_block.func,
                    block: child.into_block().unwrap(),
                },
                stacks,
                dom_tree,
                phi_mappings,
                active_path,
            );
        }

        for (alloca_inst, count) in pushed_count.iter() {
            let stack = stacks.get_mut(alloca_inst).unwrap();
            for _ in 0..*count {
                stack.pop();
            }
        }

        active_path.pop();
    }
}
