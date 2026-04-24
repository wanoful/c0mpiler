use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

use crate::ir::{
    core::{BlockId, ConstId, FunctionId, InstRef, ModuleCore, Use, ValueId},
    core_inst::{BinaryOpcode, CondBranch, ICmpCode, InstKind, PhiIncoming},
    core_int::CoreInt,
    core_value::{ConstKind, GlobalKind},
    ir_type::{Type, VoidType},
};

#[derive(Debug, Clone, PartialEq, Eq)]
enum ValueState {
    Unknown,
    Constant(CoreInt),
    Overdefined,
}

impl ValueState {
    fn merge(
        &self,
        other: &ValueState,
        merge_fn: impl Fn(&CoreInt, &CoreInt) -> Option<CoreInt>,
    ) -> ValueState {
        match (self, other) {
            (ValueState::Unknown, ValueState::Unknown) => ValueState::Unknown,
            (ValueState::Constant(c1), ValueState::Constant(c2)) => {
                merge_fn(c1, c2).map_or(ValueState::Overdefined, ValueState::Constant)
            }
            _ => ValueState::Overdefined,
        }
    }
}

impl PartialOrd for ValueState {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        use ValueState::*;
        match (self, other) {
            (Unknown, Unknown) | (Constant(_), Constant(_)) | (Overdefined, Overdefined) => {
                Some(std::cmp::Ordering::Equal)
            }
            (Unknown, _) => Some(std::cmp::Ordering::Less),
            (_, Unknown) => Some(std::cmp::Ordering::Greater),
            (Constant(_), Overdefined) => Some(std::cmp::Ordering::Less),
            (Overdefined, Constant(_)) => Some(std::cmp::Ordering::Greater),
        }
    }
}

impl ModuleCore {
    pub fn opt_sparse_conditional_constant_propagation(&mut self) {
        for id in self.functions_in_order() {
            self.func_sparse_conditional_constant_propagation(id);
        }
    }

    pub(crate) fn func_sparse_conditional_constant_propagation(&mut self, id: FunctionId) {
        let function = self.func(id);
        if function.is_declare {
            return;
        }

        let mut value_states: HashMap<InstRef, ValueState> = HashMap::new();
        let mut cfg_work_list = vec![function.entry];
        let mut inst_work_list = Vec::new();
        let mut visited_block: HashSet<_> = HashSet::new();

        while !cfg_work_list.is_empty() || !inst_work_list.is_empty() {
            while let Some(block) = cfg_work_list.pop() {
                if !visited_block.insert(block) {
                    continue;
                }

                self.func(id).blocks[block]
                    .phis
                    .clone()
                    .into_iter()
                    .for_each(|inst| {
                        self.visit_phi_inst(
                            InstRef {
                                inst,
                                func: id,
                            },
                            &mut value_states,
                            &mut inst_work_list,
                            &visited_block,
                        );
                    });

                self.func(id).blocks[block]
                    .insts
                    .clone()
                    .into_iter()
                    .for_each(|inst| {
                        self.visit_normal_inst(
                            InstRef {
                                inst,
                                func: id,
                            },
                            &mut value_states,
                            &mut inst_work_list,
                        );
                    });

                if let Some(term_inst) = self.func(id).blocks[block].terminator {
                    self.visit_term_inst(
                        InstRef {
                            inst: term_inst,
                            func: id,
                        },
                        &mut value_states,
                        &mut cfg_work_list,
                    );
                }
            }

            while let Some(inst_ref) = inst_work_list.pop() {
                let data = self.inst(inst_ref);

                if data.kind.is_phi() {
                    self.visit_phi_inst(
                        inst_ref,
                        &mut value_states,
                        &mut inst_work_list,
                        &visited_block,
                    );
                } else if data.kind.is_terminator() {
                    self.visit_term_inst(inst_ref, &mut value_states, &mut cfg_work_list);
                } else {
                    self.visit_normal_inst(inst_ref, &mut value_states, &mut inst_work_list);
                }
            }
        }
    }

    fn visit_phi_inst(
        &mut self,
        inst: InstRef,
        value_states: &mut HashMap<InstRef, ValueState>,
        inst_work_list: &mut Vec<InstRef>,
        visited_blocks: &HashSet<BlockId>,
    ) {
        let old_state = self.get_value_state(ValueId::Inst(inst), value_states);

        let inst_data = self.inst(inst);
        let incomings = inst_data.kind.as_phi().unwrap().0;
        let new_state = incomings
            .values()
            .filter_map(|PhiIncoming { block, value }| {
                if visited_blocks.contains(block) {
                    Some(value)
                } else {
                    None
                }
            })
            .fold(ValueState::Unknown, |acc, value| {
                acc.merge(&self.get_value_state(*value, value_states), |c1, c2| {
                    if c1 == c2 { Some(c1.clone()) } else { None }
                })
            });

        self.apply_value_state_change(inst, inst_work_list, old_state, new_state, value_states);
    }

    fn apply_value_state_change(
        &mut self,
        inst: InstRef,
        inst_work_list: &mut Vec<InstRef>,
        old_state: ValueState,
        new_state: ValueState,
        value_states: &mut HashMap<InstRef, ValueState>,
    ) {
        if old_state != new_state {
            if let ValueState::Constant(c) = &new_state {
                let ty = self.value_ty(ValueId::Inst(inst));
                let int_value = if let Some(int_ty) = ty.as_int() {
                    let bits = int_ty.0;
                    if c.bit_width == bits {
                        c.clone()
                    } else if c.bit_width > bits {
                        c.clone().trunc_to(bits)
                    } else {
                        c.clone().zero_extend(bits)
                    }
                } else {
                    c.clone()
                };
                let const_id = self.add_const(ty.clone(), ConstKind::Int(int_value));
                self.replace_all_uses_with(ValueId::Inst(inst), ValueId::Const(const_id));
            }

            self.value_uses(ValueId::Inst(inst))
                .iter()
                .for_each(|Use { user, .. }| {
                    inst_work_list.push(*user);
                });

            assert!(
                old_state < new_state,
                "Value state should only evolve in the direction of Unknown -> Constant -> Overdefined"
            );

            value_states.insert(inst, new_state);
        }
    }

    fn visit_normal_inst(
        &mut self,
        inst: InstRef,
        value_states: &mut HashMap<InstRef, ValueState>,
        inst_work_list: &mut Vec<InstRef>,
    ) {
        let old_state = self.get_value_state(ValueId::Inst(inst), value_states);

        let new_state =
            match &self.inst(inst).kind {
                InstKind::Binary { op, lhs, rhs } => {
                    let lhs_state = self.get_value_state(*lhs, value_states);
                    let rhs_state = self.get_value_state(*rhs, value_states);
                    let lhs_bits = self.value_int_bits(*lhs);
                    let rhs_bits = self.value_int_bits(*rhs);

                    match (lhs_bits, rhs_bits) {
                        (Some(bits), Some(rhs_bits)) if bits == rhs_bits => {
                            lhs_state.merge(&rhs_state, |a, b| {
                                let lhs = if a.bit_width == bits {
                                    a.clone()
                                } else {
                                    CoreInt::from_signed(a.as_i64(), bits)
                                };
                                let rhs = if b.bit_width == bits {
                                    b.clone()
                                } else {
                                    CoreInt::from_signed(b.as_i64(), bits)
                                };
                                Self::fold_binary_const(*op, lhs, rhs)
                            })
                        }
                        _ => ValueState::Overdefined,
                    }
                }
                InstKind::Call { .. } => ValueState::Overdefined,
                InstKind::Branch { .. } => panic!("Branch should be handled in visit_branch_inst"),
                InstKind::GetElementPtr { .. } => ValueState::Overdefined,
                InstKind::ICmp { op, lhs, rhs } => {
                    let lhs_state = self.get_value_state(*lhs, value_states);
                    let rhs_state = self.get_value_state(*rhs, value_states);
                    let lhs_bits = self.value_int_bits(*lhs);
                    let rhs_bits = self.value_int_bits(*rhs);

                    match (lhs_bits, rhs_bits) {
                        (Some(bits), Some(rhs_bits)) if bits == rhs_bits => {
                            lhs_state.merge(&rhs_state, |a, b| {
                                let lhs = if a.bit_width == bits {
                                    a.clone()
                                } else {
                                    CoreInt::from_signed(a.as_i64(), bits)
                                };
                                let rhs = if b.bit_width == bits {
                                    b.clone()
                                } else {
                                    CoreInt::from_signed(b.as_i64(), bits)
                                };
                                Some(CoreInt::new(Self::fold_icmp_const(*op, lhs, rhs) as u64, 1))
                            })
                        }
                        _ => ValueState::Overdefined,
                    }
                }
                InstKind::Phi { .. } => panic!("Phi should be handled in visit_phi_inst"),
                InstKind::Select {
                    cond,
                    then_val,
                    else_val,
                } => {
                    let cond_state = self.get_value_state(*cond, value_states);
                    let then_state = self.get_value_state(*then_val, value_states);
                    let else_state = self.get_value_state(*else_val, value_states);

                    match cond_state {
                        ValueState::Constant(c) => {
                            if c.as_u64() != 0 {
                                then_state
                            } else {
                                else_state
                            }
                        }
                        _ => then_state.merge(&else_state, |t, e| {
                            if t == e { Some(t.clone()) } else { None }
                        }),
                    }
                }
                InstKind::Trunc { value } => {
                    let state = self.get_value_state(*value, value_states);
                    let src_bits = self.value_int_bits(*value);
                    let dst_bits = self.value_int_bits(ValueId::Inst(inst));

                    match state {
                        ValueState::Constant(c) => match (src_bits, dst_bits) {
                            (Some(src), Some(dst)) if dst <= src => {
                                ValueState::Constant(if c.bit_width == src {
                                    c.trunc_to(dst)
                                } else {
                                    CoreInt::from_signed(c.as_i64(), src).trunc_to(dst)
                                })
                            }
                            _ => ValueState::Overdefined,
                        },
                        other => other,
                    }
                }
                InstKind::Zext { value } => {
                    let state = self.get_value_state(*value, value_states);
                    let src_bits = self.value_int_bits(*value);
                    let dst_bits = self.value_int_bits(ValueId::Inst(inst));

                    match state {
                        ValueState::Constant(c) => match (src_bits, dst_bits) {
                            (Some(src), Some(dst)) if src <= dst => {
                                ValueState::Constant(if c.bit_width == src {
                                    c.zero_extend(dst)
                                } else {
                                    CoreInt::from_signed(c.as_i64(), src).zero_extend(dst)
                                })
                            }
                            _ => ValueState::Overdefined,
                        },
                        other => other,
                    }
                }
                InstKind::Sext { value } => {
                    let state = self.get_value_state(*value, value_states);
                    let src_bits = self.value_int_bits(*value);
                    let dst_bits = self.value_int_bits(ValueId::Inst(inst));

                    match state {
                        ValueState::Constant(c) => match (src_bits, dst_bits) {
                            (Some(src), Some(dst)) if src <= dst => {
                                ValueState::Constant(if c.bit_width == src {
                                    c.sign_extend(dst)
                                } else {
                                    CoreInt::from_signed(c.as_i64(), src).sign_extend(dst)
                                })
                            }
                            _ => ValueState::Overdefined,
                        },
                        other => other,
                    }
                }
                _ => ValueState::Overdefined,
            };

        self.apply_value_state_change(inst, inst_work_list, old_state, new_state, value_states);
    }

    fn visit_term_inst(
        &mut self,
        inst: InstRef,
        value_states: &mut HashMap<InstRef, ValueState>,
        cfg_work_list: &mut Vec<BlockId>,
    ) {
        match self.inst(inst).kind.clone() {
            InstKind::Branch { then_block, cond } => {
                if let Some(CondBranch { cond, else_block }) = cond {
                    let cond_state = self.get_value_state(cond, value_states);
                    match cond_state {
                        ValueState::Constant(c) => {
                            let new_target = if c.as_u64() != 0 {
                                cfg_work_list.push(then_block);
                                then_block
                            } else {
                                cfg_work_list.push(else_block);
                                else_block
                            };
                            let new_inst = self.new_inst(
                                inst.func,
                                Rc::new(Type::Void(VoidType)),
                                InstKind::Branch {
                                    then_block: new_target,
                                    cond: None,
                                },
                                None,
                            );
                            self.overwrite_inst(inst, new_inst);
                        }
                        ValueState::Overdefined => {
                            cfg_work_list.push(then_block);
                            cfg_work_list.push(else_block);
                        }
                        ValueState::Unknown => {}
                    }
                } else {
                    cfg_work_list.push(then_block);
                }
            }
            InstKind::Ret { .. } | InstKind::Unreachable => {}
            _ => panic!("Not a terminator instruction"),
        }
    }

    fn get_const_value(&self, const_id: ConstId) -> ValueState {
        match self.const_data(const_id).kind {
            ConstKind::Int(ref i) => ValueState::Constant(i.clone()),
            ConstKind::Null => ValueState::Constant(self.zero_const_for_const(const_id)),
            ConstKind::Undef => ValueState::Constant(self.zero_const_for_const(const_id)),
            _ => todo!(),
        }
    }

    fn get_value_state(
        &self,
        value_id: ValueId,
        value_states: &HashMap<InstRef, ValueState>,
    ) -> ValueState {
        match value_id {
            ValueId::Inst(inst_ref) => value_states
                .get(&inst_ref)
                .cloned()
                .unwrap_or(ValueState::Unknown),
            ValueId::Arg(..) => ValueState::Overdefined,
            ValueId::Global(global_id) => match self.global(global_id).kind {
                GlobalKind::Function(..) => ValueState::Overdefined,
                GlobalKind::GlobalVariable {
                    is_constant,
                    initializer,
                } => {
                    if is_constant {
                        self.get_const_value(initializer.unwrap())
                    } else {
                        ValueState::Overdefined
                    }
                }
            },
            ValueId::Const(const_id) => self.get_const_value(const_id),
        }
    }

    fn value_int_bits(&self, value_id: ValueId) -> Option<u8> {
        self.value_ty(value_id).as_int().map(|int_ty| int_ty.0)
    }

    fn fold_binary_const(op: BinaryOpcode, lhs: CoreInt, rhs: CoreInt) -> Option<CoreInt> {
        match op {
            BinaryOpcode::Add => Some(lhs + rhs),
            BinaryOpcode::Sub => Some(lhs - rhs),
            BinaryOpcode::Mul => Some(lhs * rhs),
            BinaryOpcode::UDiv => lhs.checked_udiv(rhs),
            BinaryOpcode::SDiv => lhs.checked_sdiv(rhs),
            BinaryOpcode::URem => lhs.checked_urem(rhs),
            BinaryOpcode::SRem => lhs.checked_srem(rhs),
            BinaryOpcode::Shl => lhs.checked_shl(rhs),
            BinaryOpcode::LShr => lhs.checked_lshr(rhs),
            BinaryOpcode::AShr => lhs.checked_ashr(rhs),
            BinaryOpcode::And => Some(lhs & rhs),
            BinaryOpcode::Or => Some(lhs | rhs),
            BinaryOpcode::Xor => Some(lhs ^ rhs),
        }
    }

    fn fold_icmp_const(op: ICmpCode, lhs: CoreInt, rhs: CoreInt) -> bool {
        match op {
            ICmpCode::Eq => lhs.cmp_eq(rhs),
            ICmpCode::Ne => lhs.cmp_ne(rhs),
            ICmpCode::Ugt => lhs.cmp_ugt(rhs),
            ICmpCode::Uge => lhs.cmp_uge(rhs),
            ICmpCode::Ult => lhs.cmp_ult(rhs),
            ICmpCode::Ule => lhs.cmp_ule(rhs),
            ICmpCode::Sgt => lhs.cmp_sgt(rhs),
            ICmpCode::Sge => lhs.cmp_sge(rhs),
            ICmpCode::Slt => lhs.cmp_slt(rhs),
            ICmpCode::Sle => lhs.cmp_sle(rhs),
        }
    }

    fn zero_const_for_const(&self, const_id: ConstId) -> CoreInt {
        let bit_width = self
            .const_data(const_id)
            .ty
            .as_int()
            .map_or(64, |int_ty| int_ty.0);
        CoreInt::new(0, bit_width)
    }
}
