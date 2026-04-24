use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
    rc::Rc,
};

use crate::ir::{
    core::{ArgRef, BlockId, BlockRef, FunctionId, InstId, InstRef, ModuleCore, ValueId},
    core_inst::{CondBranch, InstKind, PhiIncoming},
    ir_type::{Type, VoidType},
};

struct CallGraph {
    succs: HashMap<FunctionId, HashSet<FunctionId>>,
    preds: HashMap<FunctionId, HashSet<FunctionId>>,
}

struct SCCGraph {
    sccs: Vec<HashSet<FunctionId>>,
    scc_id_map: HashMap<FunctionId, usize>,
    succs: HashMap<usize, HashSet<usize>>,
    preds: HashMap<usize, HashSet<usize>>,
}

impl CallGraph {
    fn build_scc(&self) -> SCCGraph {
        let dfs_order = dfs(&self.succs, self.succs.keys().cloned())
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();

        let sccs = dfs(&self.preds, dfs_order.into_iter().rev());
        let sccs = sccs
            .into_iter()
            .map(|scc| scc.into_iter().collect::<HashSet<_>>())
            .collect::<Vec<_>>();
        let mut scc_id_map = HashMap::new();
        for (scc_id, scc) in sccs.iter().enumerate() {
            for &func_id in scc {
                scc_id_map.insert(func_id, scc_id);
            }
        }

        let mut succs = HashMap::new();
        let mut preds = HashMap::new();
        for i in 0..sccs.len() {
            succs.insert(i, HashSet::new());
            preds.insert(i, HashSet::new());
        }

        for (func_id, called_funcs) in &self.succs {
            let scc_id = scc_id_map[func_id];
            for called_func_id in called_funcs {
                let called_scc_id = scc_id_map[called_func_id];
                if scc_id != called_scc_id {
                    succs.get_mut(&scc_id).unwrap().insert(called_scc_id);
                    preds.get_mut(&called_scc_id).unwrap().insert(scc_id);
                }
            }
        }

        SCCGraph {
            sccs,
            scc_id_map,
            succs,
            preds,
        }
    }
}

impl SCCGraph {
    fn topological_sort(&self) -> Vec<usize> {
        let mut degrees = HashMap::new();
        for (scc_id, preds) in &self.preds {
            degrees.insert(*scc_id, preds.len());
        }

        let mut work_list = degrees
            .iter()
            .filter_map(|(&scc_id, &degree)| if degree == 0 { Some(scc_id) } else { None })
            .collect::<Vec<_>>();
        let mut order = Vec::new();

        while let Some(scc_id) = work_list.pop() {
            order.push(scc_id);
            for &succ in &self.succs[&scc_id] {
                *degrees.get_mut(&succ).unwrap() -= 1;
                if *degrees.get(&succ).unwrap() == 0 {
                    work_list.push(succ);
                }
            }
        }

        order
    }
}

fn dfs<T>(succs: &HashMap<T, HashSet<T>>, dfs_order: impl Iterator<Item = T>) -> Vec<Vec<T>>
where
    T: Eq + Hash + Copy,
{
    let mut visited = HashSet::new();
    let mut order = Vec::new();

    fn dfs_visit<T>(
        func_id: T,
        visited: &mut HashSet<T>,
        order: &mut Vec<T>,
        succs: &HashMap<T, HashSet<T>>,
    ) where
        T: Eq + Hash + Copy,
    {
        if !visited.insert(func_id) {
            return;
        }
        for &succ in &succs[&func_id] {
            dfs_visit(succ, visited, order, succs);
        }
        order.push(func_id);
    }

    for func_id in dfs_order {
        let mut o = Vec::new();
        dfs_visit(func_id, &mut visited, &mut o, succs);
        order.push(o);
    }

    order
}

impl ModuleCore {
    pub fn opt_function_inline(&mut self) {
        let call_graph = self.build_call_graph();
        let scc_graph = call_graph.build_scc();
        let topological_order = scc_graph.topological_sort();

        // for (i, &scc_id) in topological_order.iter().enumerate() {
        //     eprintln!("SCC {}: {:?}", i, scc_graph.sccs[scc_id].iter().map(|s| {self.func(*s).name.clone()}).collect::<Vec<_>>());
        // }

        for scc_id in topological_order.into_iter().rev() {
            let scc = &scc_graph.sccs[scc_id];
            for &func_id in scc {
                let func = self.func(func_id);
                let mut inlinable_insts = Vec::new();
                for block in func.block_order.iter() {
                    for inst in func.blocks[*block].insts.iter() {
                        let i = self.inst(InstRef {
                            func: func_id,
                            inst: *inst,
                        });
                        if let Some(_) = i.kind.as_call() {
                            let score = self.analyze_inlinability(
                                InstRef {
                                    func: func_id,
                                    inst: *inst,
                                },
                                &scc_graph,
                            );
                            if score > 0 {
                                inlinable_insts.push(InstRef {
                                    func: func_id,
                                    inst: *inst,
                                });
                            }
                        }
                    }
                }
                self.inline_calls(func_id, inlinable_insts);

                self.func_cfg_simplify(func_id);
                self.func_sparse_conditional_constant_propagation(func_id);
                self.func_cfg_simplify(func_id);
                self.func_aggressive_dead_code_elimination(func_id);
                self.func_cfg_simplify(func_id);
            }
        }
    }

    fn analyze_inlinability(&self, inst: InstRef, scc: &SCCGraph) -> i64 {
        let inst_data = self.inst(inst);
        let (callee, args) = inst_data.kind.as_call().unwrap();
        if scc.scc_id_map[&inst.func] == scc.scc_id_map[&*callee] {
            return i64::MIN;
        }
        let callee_func = self.func(*callee);
        if callee_func.is_declare {
            return i64::MIN;
        }

        // Is there any situation that can never inline?

        let mut score = 30;

        for block in callee_func.block_order.iter() {
            for inst in callee_func.blocks[*block].insts.iter() {
                let i = self.inst(InstRef {
                    func: *callee,
                    inst: *inst,
                });
                match i.kind {
                    InstKind::Call { .. } => score -= 10,
                    InstKind::Branch { .. } => score -= 2,
                    _ => score -= 1,
                }
            }
        }

        score += args.len() as i64;

        score
    }

    fn build_call_graph(&self) -> CallGraph {
        let mut succs = HashMap::new();
        let mut preds = HashMap::new();

        for func_id in self.functions_in_order() {
            let called_funcs = self.get_called_functions(func_id);
            succs.insert(func_id, called_funcs);
            preds.entry(func_id).or_default();
            for called_func_id in &succs[&func_id] {
                preds
                    .entry(*called_func_id)
                    .or_insert_with(HashSet::new)
                    .insert(func_id);
            }
        }

        CallGraph { succs, preds }
    }

    fn get_called_functions(&self, func_id: FunctionId) -> HashSet<FunctionId> {
        let mut called_funcs = HashSet::new();
        let func = self.func(func_id);

        for block in func.block_order.iter() {
            for inst in func.blocks[*block].insts.iter() {
                let i = self.inst(InstRef {
                    func: func_id,
                    inst: *inst,
                });
                if let Some((&callee, ..)) = i.kind.as_call() {
                    called_funcs.insert(callee);
                }
            }
        }

        called_funcs
    }

    fn inline_calls(&mut self, caller: FunctionId, insts: Vec<InstRef>) {
        for inst in insts {
            self.inline_call(caller, inst);
        }
    }

    fn inline_call(&mut self, caller: FunctionId, call_inst: InstRef) {
        let (&callee, args) = self.inst(call_inst).kind.as_call().unwrap();
        let callee_name = self.func(callee).name.clone();
        let inst_block = self.inst(call_inst).parent.unwrap();

        let mut block_mapping = HashMap::new();
        let mut inst_mapping = HashMap::new();
        let arg_mapping = self
            .args_in_order(callee)
            .into_iter()
            .zip(args.iter().cloned())
            .collect::<HashMap<_, _>>();

        let ret_target_block = self.split_block_at(call_inst, false);
        let mut rets = Vec::new();

        for block in self.func(callee).block_order.clone() {
            let block_name = if let Some(n) = &self.func(callee).blocks[block].name {
                n.as_str()
            } else {
                ".anon"
            };
            let new_block_name = format!("inline.{}.{}", callee_name, block_name);
            let new_block = self.append_block(caller, Some(new_block_name));
            block_mapping.insert(block, new_block.block);
        }

        for block in self.func(callee).block_order.clone().iter() {
            let target_block = block_mapping[block];
            let target_block_ref = BlockRef {
                func: caller,
                block: target_block,
            };
            let block_data = &self.func(callee).blocks[*block];
            let phis = block_data.phis.clone();
            let insts = block_data.insts.clone();
            let terminator = block_data.terminator;

            let mut copy_inst_fn = |inst_id| {
                let old_ref = InstRef {
                    func: callee,
                    inst: inst_id,
                };
                let old_inst = self.inst(old_ref);
                let is_phi = old_inst.kind.is_phi();
                let is_terminator = old_inst.kind.is_terminator();
                let inst_ty = old_inst.ty.clone();

                let (cloned_kind, ret_value) = self.clone_inst_kind(
                    old_inst.kind.clone(),
                    &block_mapping,
                    &mut inst_mapping,
                    &arg_mapping,
                    ret_target_block,
                );

                if let Some(value) = ret_value {
                    rets.push((target_block_ref, value));
                }

                let new_inst = self.new_inst(caller, inst_ty, cloned_kind, None);
                if is_phi {
                    self.append_phi(target_block_ref, new_inst);
                } else if is_terminator {
                    self.set_terminator(target_block_ref, new_inst);
                } else {
                    self.append_inst(target_block_ref, new_inst);
                }
                if let Some(value) = inst_mapping.insert(old_ref, ValueId::Inst(new_inst)) {
                    self.replace_all_uses_with(value, ValueId::Inst(new_inst));
                }
            };

            for phi in phis.iter() {
                copy_inst_fn(*phi);
            }
            for inst in insts.iter() {
                copy_inst_fn(*inst);
            }
            if let Some(terminator) = terminator {
                copy_inst_fn(terminator);
            }
        }

        let call_to_jump = self.new_inst(
            caller,
            Rc::new(Type::Void(VoidType)),
            InstKind::Branch {
                then_block: block_mapping[&self.func(callee).entry],
                cond: None,
            },
            None,
        );
        self.set_terminator(inst_block, call_to_jump);

        if !rets.is_empty() {
            let phi_inst_kind = InstKind::Phi {
                incomings: HashMap::new(),
                idx: 0,
            };
            let ty = self.value_ty(rets[0].1).clone();
            let phi_inst = self.new_inst(caller, ty, phi_inst_kind, None);
            self.append_phi(ret_target_block, phi_inst);
            for ret in rets {
                self.phi_add_incoming(phi_inst, ret.0, ret.1);
            }
            self.replace_all_uses_with(ValueId::Inst(call_inst), ValueId::Inst(phi_inst));
        }

        self.erase_inst_from_parent(call_inst);
    }

    fn clone_inst_kind(
        &mut self,
        inst_kind: InstKind,
        block_mapping: &HashMap<BlockId, BlockId>,
        inst_mapping: &mut HashMap<InstRef, ValueId>,
        arg_mapping: &HashMap<ArgRef, ValueId>,
        ret_target_block: BlockRef,
    ) -> (InstKind, Option<ValueId>) {
        fn get_value_mapping(
            value: ValueId,
            inst_mapping: &mut HashMap<InstRef, ValueId>,
            arg_mapping: &HashMap<ArgRef, ValueId>,
            module: &mut ModuleCore,
        ) -> ValueId {
            match value {
                ValueId::Inst(inst_ref) => {
                    inst_mapping.entry(inst_ref).or_insert_with(|| {
                        ValueId::Const(module.add_undef_const(module.inst(inst_ref).ty.clone()))
                    });
                    inst_mapping[&inst_ref]
                }
                ValueId::Arg(arg_ref) => arg_mapping[&arg_ref],
                ValueId::Global(..) | ValueId::Const(..) => value,
            }
        }

        (
            match &inst_kind {
                InstKind::Binary { op, lhs, rhs } => InstKind::Binary {
                    op: *op,
                    lhs: get_value_mapping(*lhs, inst_mapping, arg_mapping, self),
                    rhs: get_value_mapping(*rhs, inst_mapping, arg_mapping, self),
                },
                InstKind::Call { func, args } => InstKind::Call {
                    func: *func,
                    args: args
                        .iter()
                        .map(|&arg| get_value_mapping(arg, inst_mapping, arg_mapping, self))
                        .collect(),
                },
                InstKind::Branch { then_block, cond } => InstKind::Branch {
                    then_block: block_mapping[then_block],
                    cond: cond.as_ref().map(|cond| CondBranch {
                        cond: get_value_mapping(cond.cond, inst_mapping, arg_mapping, self),
                        else_block: block_mapping[&cond.else_block],
                    }),
                },
                InstKind::GetElementPtr {
                    base_ty,
                    base,
                    indices,
                } => InstKind::GetElementPtr {
                    base_ty: base_ty.clone(),
                    base: get_value_mapping(*base, inst_mapping, arg_mapping, self),
                    indices: indices
                        .iter()
                        .map(|&index| get_value_mapping(index, inst_mapping, arg_mapping, self))
                        .collect(),
                },
                InstKind::Alloca { ty } => InstKind::Alloca { ty: ty.clone() },
                InstKind::Load { ptr } => InstKind::Load {
                    ptr: get_value_mapping(*ptr, inst_mapping, arg_mapping, self),
                },
                InstKind::Ret { value } => {
                    return (
                        InstKind::Branch {
                            then_block: ret_target_block.block,
                            cond: None,
                        },
                        value.map(|v| get_value_mapping(v, inst_mapping, arg_mapping, self)),
                    );
                }
                InstKind::Store { value, ptr } => InstKind::Store {
                    value: get_value_mapping(*value, inst_mapping, arg_mapping, self),
                    ptr: get_value_mapping(*ptr, inst_mapping, arg_mapping, self),
                },
                InstKind::ICmp { op, lhs, rhs } => InstKind::ICmp {
                    op: *op,
                    lhs: get_value_mapping(*lhs, inst_mapping, arg_mapping, self),
                    rhs: get_value_mapping(*rhs, inst_mapping, arg_mapping, self),
                },
                InstKind::Phi { incomings, idx } => InstKind::Phi {
                    incomings: incomings
                        .iter()
                        .map(|(id, PhiIncoming { block, value })| {
                            (
                                *id,
                                PhiIncoming {
                                    block: block_mapping[block],
                                    value: get_value_mapping(
                                        *value,
                                        inst_mapping,
                                        arg_mapping,
                                        self,
                                    ),
                                },
                            )
                        })
                        .collect(),
                    idx: *idx,
                },
                InstKind::Select {
                    cond,
                    then_val,
                    else_val,
                } => InstKind::Select {
                    cond: get_value_mapping(*cond, inst_mapping, arg_mapping, self),
                    then_val: get_value_mapping(*then_val, inst_mapping, arg_mapping, self),
                    else_val: get_value_mapping(*else_val, inst_mapping, arg_mapping, self),
                },
                InstKind::PtrToInt { ptr } => InstKind::PtrToInt {
                    ptr: get_value_mapping(*ptr, inst_mapping, arg_mapping, self),
                },
                InstKind::Trunc { value } => InstKind::Trunc {
                    value: get_value_mapping(*value, inst_mapping, arg_mapping, self),
                },
                InstKind::Zext { value } => InstKind::Zext {
                    value: get_value_mapping(*value, inst_mapping, arg_mapping, self),
                },
                InstKind::Sext { value } => InstKind::Sext {
                    value: get_value_mapping(*value, inst_mapping, arg_mapping, self),
                },
                InstKind::Unreachable => InstKind::Unreachable,
            },
            None,
        )
    }
}
