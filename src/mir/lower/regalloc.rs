use std::collections::{HashMap, HashSet};

use crate::mir::{
    BlockId, LivenessInfo, LoweringTarget, MachineFunction, Register, StackSlotKind, TargetInst,
    VRegId, lower::Lowerer,
};

struct InterferenceGraph<T: LoweringTarget> {
    edges: HashMap<VRegId, HashSet<VRegId>>,
    forbidden_phys: HashMap<VRegId, HashSet<T::PhysicalReg>>,
}

#[derive(Debug, Clone, Copy, Default)]
struct VRegStats {
    use_weight: u64,
    def_weight: u64,
    call_cross_weight: u64,
}

impl VRegStats {
    fn spill_cost(self) -> u64 {
        // Uses become reloads and defs become stores. Keep call-live values a
        // little more expensive too, because they are often long-lived state.
        self.use_weight * 4 + self.def_weight * 5 + self.call_cross_weight * 2 + 1
    }
}

impl<T: LoweringTarget> InterferenceGraph<T> {
    fn build(machine_function: &MachineFunction<T>, liveness_info: &LivenessInfo<T>) -> Self {
        let mut edges: HashMap<VRegId, HashSet<VRegId>> = HashMap::new();
        let mut forbidden_phys: HashMap<VRegId, HashSet<T::PhysicalReg>> = HashMap::new();

        for block in machine_function.blocks.iter() {
            for (index, inst) in block.instructions.iter().enumerate() {
                let live_after = liveness_info.get_live_after(block.id, index);

                let defs: Vec<_> = inst.def_regs();
                let conflict_regs = inst.def_conflict_regs();

                for def in defs.iter() {
                    if let Register::Virtual(vreg_id) = def {
                        edges.entry(*vreg_id).or_default();
                    }
                }

                for def in defs.iter() {
                    let conflicts = conflict_regs.get(def).cloned().unwrap_or_default();
                    for x in live_after.iter().chain(conflicts.iter()) {
                        use super::Register::*;
                        match (def, x) {
                            (Virtual(v1), Virtual(v2)) => {
                                if v1 != v2 {
                                    edges.entry(*v1).or_default().insert(*v2);
                                    edges.entry(*v2).or_default().insert(*v1);
                                }
                            }
                            (Virtual(vreg_id), Physical(phy))
                            | (Physical(phy), Virtual(vreg_id)) => {
                                forbidden_phys.entry(*vreg_id).or_default().insert(*phy);
                            }
                            (Physical(_), Physical(_)) => {}
                        }
                    }
                }
            }
        }

        InterferenceGraph {
            edges,
            forbidden_phys,
        }
    }

    fn available_regs(&self, vreg_id: VRegId) -> Vec<T::PhysicalReg> {
        let all_regs = T::get_allocatable_regs();
        if let Some(forbidden) = self.forbidden_phys.get(&vreg_id) {
            all_regs
                .into_iter()
                .filter(|x| !forbidden.contains(x))
                .collect()
        } else {
            all_regs
        }
    }

    fn simplify(&self, vreg_stats: &HashMap<VRegId, VRegStats>) -> Vec<VRegId> {
        let mut stack = Vec::new();
        let mut degrees: HashMap<VRegId, (usize, usize)> = self
            .edges
            .iter()
            .map(|(id, neighbor)| (*id, (neighbor.len(), self.available_regs(*id).len())))
            .collect();

        while !degrees.is_empty() {
            let node = if let Some((node, _)) = degrees
                .iter()
                .filter(|(_, (degree, k))| degree < k)
                .min_by_key(|(node, (degree, k))| (*degree, *k, node.0))
            {
                *node
            } else {
                self.select_spill_candidate(&degrees, vreg_stats)
            };
            degrees.remove(&node);
            stack.push(node);
            self.edges[&node].iter().for_each(|neighbor| {
                if let Some((degree, _)) = degrees.get_mut(neighbor) {
                    *degree -= 1;
                }
            });
        }

        stack
    }

    fn select_spill_candidate(
        &self,
        degrees: &HashMap<VRegId, (usize, usize)>,
        vreg_stats: &HashMap<VRegId, VRegStats>,
    ) -> VRegId {
        *degrees
            .iter()
            .min_by(|(lhs_id, (lhs_degree, _)), (rhs_id, (rhs_degree, _))| {
                let lhs_cost = vreg_stats
                    .get(lhs_id)
                    .copied()
                    .unwrap_or_default()
                    .spill_cost();
                let rhs_cost = vreg_stats
                    .get(rhs_id)
                    .copied()
                    .unwrap_or_default()
                    .spill_cost();
                let lhs_degree = (*lhs_degree as u128) + 1;
                let rhs_degree = (*rhs_degree as u128) + 1;

                // Lower cost per interference edge is the more profitable spill.
                let lhs_scaled = (lhs_cost as u128) * rhs_degree;
                let rhs_scaled = (rhs_cost as u128) * lhs_degree;
                lhs_scaled
                    .cmp(&rhs_scaled)
                    .then_with(|| rhs_degree.cmp(&lhs_degree))
                    .then_with(|| lhs_id.0.cmp(&rhs_id.0))
            })
            .unwrap()
            .0
    }
}

impl<T: LoweringTarget> Lowerer<T> {
    fn compute_block_weights(
        &self,
        machine_function: &MachineFunction<T>,
    ) -> HashMap<BlockId, u64> {
        let cfg = self.compute_cfg(machine_function);
        let block_order = machine_function
            .blocks
            .iter()
            .enumerate()
            .map(|(index, block)| (block.id, index))
            .collect::<HashMap<_, _>>();

        let mut predecessors: HashMap<BlockId, HashSet<BlockId>> = HashMap::new();
        for block in machine_function.blocks.iter() {
            predecessors.entry(block.id).or_default();
        }
        for (block, succs) in cfg.succs.iter() {
            for succ in succs {
                predecessors.entry(*succ).or_default().insert(*block);
            }
        }

        let mut loop_depths: HashMap<BlockId, usize> = machine_function
            .blocks
            .iter()
            .map(|block| (block.id, 0))
            .collect();

        for block in machine_function.blocks.iter() {
            let block_index = block_order[&block.id];
            for succ in cfg.succs.get(&block.id).into_iter().flatten() {
                if block_order[succ] > block_index {
                    continue;
                }

                let mut loop_blocks = HashSet::from([*succ]);
                let mut worklist = vec![block.id];
                while let Some(current) = worklist.pop() {
                    if !loop_blocks.insert(current) {
                        continue;
                    }
                    for pred in predecessors.get(&current).into_iter().flatten() {
                        worklist.push(*pred);
                    }
                }

                for loop_block in loop_blocks {
                    *loop_depths.entry(loop_block).or_default() += 1;
                }
            }
        }

        loop_depths
            .into_iter()
            .map(|(block, depth)| (block, 10_u64.pow(depth.min(4) as u32)))
            .collect()
    }

    fn collect_vreg_stats(
        &self,
        machine_function: &MachineFunction<T>,
        liveness_info: &LivenessInfo<T>,
        block_weights: &HashMap<BlockId, u64>,
    ) -> HashMap<VRegId, VRegStats> {
        let mut stats = HashMap::new();

        for block in machine_function.blocks.iter() {
            let weight = block_weights.get(&block.id).copied().unwrap_or(1);
            for (index, inst) in block.instructions.iter().enumerate() {
                for reg in inst.use_regs() {
                    if let Register::Virtual(vreg_id) = reg {
                        stats
                            .entry(vreg_id)
                            .or_insert_with(VRegStats::default)
                            .use_weight += weight;
                    }
                }
                for reg in inst.def_regs() {
                    if let Register::Virtual(vreg_id) = reg {
                        stats
                            .entry(vreg_id)
                            .or_insert_with(VRegStats::default)
                            .def_weight += weight;
                    }
                }
                if inst.is_call() {
                    for reg in liveness_info.get_live_after(block.id, index) {
                        if let Register::Virtual(vreg_id) = reg {
                            stats
                                .entry(*vreg_id)
                                .or_insert_with(VRegStats::default)
                                .call_cross_weight += weight;
                        }
                    }
                }
            }
        }

        stats
    }

    fn collect_move_pairs(
        &self,
        machine_function: &MachineFunction<T>,
        block_weights: &HashMap<BlockId, u64>,
    ) -> HashMap<Register<T::PhysicalReg>, HashMap<Register<T::PhysicalReg>, u64>> {
        let mut move_pairs: HashMap<
            Register<T::PhysicalReg>,
            HashMap<Register<T::PhysicalReg>, u64>,
        > = HashMap::new();
        for block in machine_function.blocks.iter() {
            let weight = block_weights.get(&block.id).copied().unwrap_or(1);
            for inst in block.instructions.iter() {
                if let Some((src, dst)) = inst.as_move() {
                    *move_pairs.entry(src).or_default().entry(dst).or_default() += weight;
                    *move_pairs.entry(dst).or_default().entry(src).or_default() += weight;
                }
            }
        }
        move_pairs
    }

    fn choose_physical_reg(
        &self,
        vreg_id: VRegId,
        available_regs: &[T::PhysicalReg],
        assigned_regs: &HashMap<VRegId, T::PhysicalReg>,
        used_callee_saved: &HashSet<T::PhysicalReg>,
        move_pairs: &HashMap<Register<T::PhysicalReg>, HashMap<Register<T::PhysicalReg>, u64>>,
    ) -> Option<T::PhysicalReg> {
        let reg_order = T::get_allocatable_regs()
            .into_iter()
            .enumerate()
            .map(|(index, reg)| (reg, index))
            .collect::<HashMap<_, _>>();

        let mut preferred_weights: HashMap<T::PhysicalReg, u64> = HashMap::new();
        if let Some(preferred) = move_pairs.get(&Register::Virtual(vreg_id)) {
            for (reg, weight) in preferred {
                match reg {
                    Register::Virtual(other_vreg) => {
                        if let Some(assigned) = assigned_regs.get(other_vreg) {
                            *preferred_weights.entry(*assigned).or_default() += *weight;
                        }
                    }
                    Register::Physical(phy) => {
                        *preferred_weights.entry(*phy).or_default() += *weight;
                    }
                }
            }
        }

        available_regs.iter().copied().min_by_key(|reg| {
            let preference_bonus = preferred_weights
                .get(reg)
                .copied()
                .unwrap_or(0)
                .saturating_mul(100) as i64;
            let callee_saved_penalty = if T::is_callee_saved(*reg) {
                if used_callee_saved.contains(reg) {
                    1_000
                } else {
                    10_000
                }
            } else {
                0
            };
            (
                callee_saved_penalty - preference_bonus,
                reg_order.get(reg).copied().unwrap_or(usize::MAX),
                *reg,
            )
        })
    }

    fn compute_spill(
        &self,
        machine_function: &MachineFunction<T>,
    ) -> (Vec<VRegId>, HashMap<VRegId, T::PhysicalReg>) {
        let liveness_info = self.liveness_analysis(machine_function);
        let block_weights = self.compute_block_weights(machine_function);
        let vreg_stats = self.collect_vreg_stats(machine_function, &liveness_info, &block_weights);

        let graph = InterferenceGraph::build(machine_function, &liveness_info);
        let move_pairs = self.collect_move_pairs(machine_function, &block_weights);
        let stack = graph.simplify(&vreg_stats);

        let mut assigned_regs: HashMap<VRegId, T::PhysicalReg> = HashMap::new();
        let mut used_callee_saved = HashSet::new();
        let mut spill_candidates = Vec::new();

        for vreg_id in stack.into_iter().rev() {
            let mut available_regs = graph.available_regs(vreg_id);

            available_regs.retain(|reg| {
                graph.edges[&vreg_id]
                    .iter()
                    .all(|neighbor| match assigned_regs.get(neighbor) {
                        Some(assigned) => assigned != reg,
                        None => true,
                    })
            });

            if let Some(reg) = self.choose_physical_reg(
                vreg_id,
                &available_regs,
                &assigned_regs,
                &used_callee_saved,
                &move_pairs,
            ) {
                if T::is_callee_saved(reg) {
                    used_callee_saved.insert(reg);
                }
                assigned_regs.insert(vreg_id, reg);
            } else {
                spill_candidates.push(vreg_id);
            }
        }
        (spill_candidates, assigned_regs)
    }

    pub(crate) fn register_allocation(&self, machine_function: &mut MachineFunction<T>) {
        let (mut spill, mut assigned_regs) = self.compute_spill(machine_function);
        while !spill.is_empty() {
            self.spill_vreg(spill, machine_function);
            (spill, assigned_regs) = self.compute_spill(machine_function);
        }

        let assigned_regs = assigned_regs
            .into_iter()
            .map(|(vreg_id, phy)| (vreg_id, Register::Physical(phy)))
            .collect::<HashMap<_, _>>();
        for block in machine_function.blocks.iter_mut() {
            for inst in block.instructions.iter_mut() {
                *inst = inst.rewrite_vreg(&assigned_regs, &assigned_regs);
            }
        }

        self.callee_saved_scan(machine_function);
    }

    fn callee_saved_scan(&self, machine_function: &mut MachineFunction<T>) {
        let mut used_callee_saved = HashSet::new();
        let mut need_save_ra = false;
        for block in machine_function.blocks.iter() {
            for inst in block.instructions.iter() {
                need_save_ra |= inst.is_call();
                for r in inst.def_regs().iter() {
                    if let Register::Physical(phy) = r
                        && T::is_callee_saved(*phy)
                    {
                        used_callee_saved.insert(*phy);
                    }
                }
            }
        }
        machine_function.frame_info.used_callee_saved = used_callee_saved;
        machine_function.frame_info.need_save_ra = need_save_ra;
    }

    fn unique_spilled_vregs(
        regs: Vec<Register<T::PhysicalReg>>,
        spilled: &HashSet<VRegId>,
    ) -> Vec<VRegId> {
        let mut seen = HashSet::new();
        regs.into_iter()
            .filter_map(|r| match r {
                Register::Virtual(v) if spilled.contains(&v) && seen.insert(v) => Some(v),
                _ => None,
            })
            .collect()
    }

    fn spill_vreg(&self, vreg_ids: Vec<VRegId>, machine_function: &mut MachineFunction<T>) {
        let spilled = vreg_ids.iter().copied().collect::<HashSet<_>>();
        let slots = vreg_ids
            .iter()
            .map(|vreg_id| {
                (
                    *vreg_id,
                    machine_function.new_stack_slot(
                        T::WORD_SIZE,
                        T::WORD_SIZE,
                        StackSlotKind::Spill,
                    ),
                )
            })
            .collect::<HashMap<_, _>>();

        for block_index in 0..machine_function.blocks.len() {
            let old_insts = std::mem::take(&mut machine_function.blocks[block_index].instructions);
            let mut new_insts = Vec::new();

            for inst in old_insts.iter() {
                let uses_spilled = Self::unique_spilled_vregs(inst.use_regs(), &spilled);
                let defs_spilled = Self::unique_spilled_vregs(inst.def_regs(), &spilled);

                let mut use_map = HashMap::new();
                let mut def_map = HashMap::new();

                for vreg_id in uses_spilled.iter() {
                    let temp_in = Register::Virtual(machine_function.new_vreg());
                    new_insts.push(T::emit_load_stack_slot(temp_in, slots[vreg_id]));
                    use_map.insert(*vreg_id, temp_in);
                }

                for vreg_id in defs_spilled.iter() {
                    let temp_out = Register::Virtual(machine_function.new_vreg());
                    def_map.insert(*vreg_id, temp_out);
                }

                let rewritten = inst.rewrite_vreg(&use_map, &def_map);
                new_insts.push(rewritten);

                for vreg_id in defs_spilled.iter() {
                    let temp_out = def_map[vreg_id];
                    let rt = Register::Virtual(machine_function.new_vreg());
                    new_insts.push(T::emit_store_stack_slot(temp_out, slots[vreg_id], rt));
                }
            }

            machine_function.blocks[block_index].instructions = new_insts;
        }
    }
}
