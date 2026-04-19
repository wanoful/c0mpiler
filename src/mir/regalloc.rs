use std::collections::{HashMap, HashSet};

use crate::mir::{
    LivenessInfo, LoweringTarget, MachineFunction, Register, StackSlotKind, TargetInst, VRegId,
    lower::{FunctionLoweringState, Lowerer},
};

struct InterferenceGraph<T: LoweringTarget> {
    edges: HashMap<VRegId, HashSet<VRegId>>,
    forbidden_phys: HashMap<VRegId, HashSet<T::PhysicalReg>>,
}

impl<T: LoweringTarget> InterferenceGraph<T> {
    fn build(machine_function: &MachineFunction<T>, liveness_info: &LivenessInfo<T>) -> Self {
        let mut edges: HashMap<VRegId, HashSet<VRegId>> = HashMap::new();
        let mut forbidden_phys: HashMap<VRegId, HashSet<T::PhysicalReg>> = HashMap::new();

        for block in machine_function.blocks.iter() {
            for (index, inst) in block.instructions.iter().enumerate() {
                let live_after = liveness_info.get_live_after(block.id, index);

                let defs: Vec<_> = inst.def_regs();

                for def in defs.iter() {
                    if let Register::Virtual(vreg_id) = def {
                        edges.entry(*vreg_id).or_default();
                    }
                }

                for def in defs.iter() {
                    for x in live_after.iter() {
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

    fn degree(&self, vreg_id: VRegId) -> usize {
        self.edges
            .get(&vreg_id)
            .map_or(0, |neighbors| neighbors.len())
    }

    fn avaliable_regs(&self, vreg_id: VRegId) -> Vec<T::PhysicalReg> {
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

    fn simplify(&self) -> Vec<VRegId> {
        let mut stack = Vec::new();
        let mut degrees: HashMap<VRegId, (usize, usize)> = self
            .edges
            .iter()
            .map(|(id, neighbor)| (*id, (neighbor.len(), self.avaliable_regs(*id).len())))
            .collect();

        while !degrees.is_empty() {
            let node = if let Some((node, _)) = degrees.iter().find(|(_, (degree, k))| degree < k) {
                *node
            } else {
                *degrees
                    .iter()
                    .max_by_key(|(_, (degree, _))| *degree)
                    .unwrap()
                    .0
            };
            degrees.remove(&node);
            stack.push(node);
            self.edges[&node].iter().for_each(|neighbor| {
                degrees.get_mut(neighbor).map(|(degree, _)| *degree -= 1);
            });
        }

        stack
    }
}

impl<T: LoweringTarget> Lowerer<T> {
    fn compute_spill(
        &self,
        machine_function: &MachineFunction<T>,
    ) -> (HashSet<VRegId>, HashMap<VRegId, T::PhysicalReg>) {
        let liveness_info = self.liveness_analysis(machine_function);

        let graph = InterferenceGraph::build(machine_function, &liveness_info);
        let stack = graph.simplify();

        let mut assigned_regs: HashMap<VRegId, T::PhysicalReg> = HashMap::new();
        let mut spill = HashSet::new();

        for vreg_id in stack.into_iter().rev() {
            let mut available_regs: HashSet<T::PhysicalReg> =
                HashSet::from_iter(graph.avaliable_regs(vreg_id));
            if let Some(forbidden_regs) = graph.forbidden_phys.get(&vreg_id) {
                available_regs = available_regs.difference(forbidden_regs).cloned().collect();
            }

            for neighbor in graph.edges[&vreg_id].iter() {
                if let Some(assigned) = assigned_regs.get(neighbor) {
                    available_regs.remove(assigned);
                }
            }

            if let Some(&reg) = available_regs.iter().next() {
                assigned_regs.insert(vreg_id, reg);
            } else {
                spill.insert(vreg_id);
            }
        }
        (spill, assigned_regs)
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
    }

    fn spill_vreg(&self, vreg_ids: HashSet<VRegId>, machine_function: &mut MachineFunction<T>) {
        let slots = vreg_ids
            .iter()
            .map(|id| {
                (
                    id,
                    machine_function.new_stack_slot(
                        T::WORD_SIZE,
                        T::WORD_SIZE,
                        StackSlotKind::Spill,
                    ),
                )
            })
            .collect::<HashMap<_, _>>();

        for block in machine_function.blocks.iter_mut() {
            let mut new_insts = Vec::new();

            for inst in block.instructions.iter() {
                let uses_spilled: HashSet<_> = inst
                    .use_regs()
                    .iter()
                    .filter_map(|r| match r {
                        super::Register::Virtual(vreg_id) => {
                            slots.contains_key(vreg_id).then_some(vreg_id)
                        }
                        super::Register::Physical(_) => None,
                    })
                    .cloned()
                    .collect();
                let defs_spilled: HashSet<_> = inst
                    .def_regs()
                    .iter()
                    .filter_map(|r| match r {
                        super::Register::Virtual(vreg_id) => {
                            slots.contains_key(vreg_id).then_some(vreg_id)
                        }
                        super::Register::Physical(_) => None,
                    })
                    .cloned()
                    .collect();

                let mut use_map = HashMap::new();
                let mut def_map = HashMap::new();

                for use_spilled in uses_spilled.iter() {
                    let temp_in = machine_function.vreg_counter.next();
                    let slot = slots[use_spilled];
                    new_insts.push(T::emit_load_stack_slot(Register::Virtual(temp_in), slot));
                    use_map.insert(*use_spilled, Register::Virtual(temp_in));
                }

                for def_spilled in defs_spilled.iter() {
                    let temp_out = machine_function.vreg_counter.next();
                    def_map.insert(*def_spilled, Register::Virtual(temp_out));
                }

                let rewritten = inst.rewrite_vreg(&use_map, &def_map);
                new_insts.push(rewritten);

                for def_spilled in defs_spilled.iter() {
                    let temp_out = def_map[def_spilled];
                    let slot = slots[def_spilled];
                    new_insts.push(T::emit_store_stack_slot(temp_out, slot));
                }
            }

            block.instructions = new_insts;
        }
    }
}
