use std::collections::{HashMap, HashSet};

use crate::mir::{
    LivenessInfo, LoweringTarget, MachineFunction, TargetInst, VRegId, lower::{FunctionLoweringState, Lowerer},
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
                                edges.entry(*vreg_id).or_default();
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
    fn compute_spill(&self, machine_function: &MachineFunction<T>) -> HashSet<VRegId> {
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
        spill
    }

    pub(crate) fn register_allocation(&self, machine_function: &mut MachineFunction<T>, state: &mut FunctionLoweringState) {
        let mut spill = self.compute_spill(machine_function);
        while !spill.is_empty() {
            for vreg_id in spill.iter() {
                todo!()
            } 
            spill = self.compute_spill(machine_function);
        }
    }

    fn spill_vreg(&self, vreg_id: VRegId, machine_function: &mut MachineFunction<T>) {

    }
}
