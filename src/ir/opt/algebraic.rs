use std::collections::HashMap;

use crate::ir::{
    core::{FunctionId, InstRef, ModuleCore, ValueId},
    core_inst::{BinaryOpcode, ICmpCode, InstKind, PhiIncoming},
    core_int::CoreInt,
    core_value::{ConstKind, GlobalKind},
    ir_type::TypePtr,
};

impl ModuleCore {
    pub fn opt_algebraic_simplification(&mut self) {
        for id in self.functions_in_order() {
            self.func_algebraic_simplification(id);
        }
    }

    fn func_algebraic_simplification(&mut self, id: FunctionId) {
        let function = self.func(id);
        if function.is_declare {
            return;
        }

        let mut work_list: Vec<InstRef> = function
            .insts
            .keys()
            .map(|inst| InstRef { func: id, inst })
            .collect();

        while let Some(inst_ref) = work_list.pop() {
            if !self.inst_exists_alg(inst_ref) {
                continue;
            }

            if let Some(result) = self.try_simplify_inst(inst_ref) {
                self.replace_all_uses_with(ValueId::Inst(inst_ref), result);

                let users: Vec<_> = self.value_uses(result).iter().map(|u| u.user).collect();
                work_list.extend(users);

                if self.inst_has_no_uses_alg(inst_ref) {
                    self.erase_inst_from_parent(inst_ref);
                }
            }
        }
    }

    fn inst_exists_alg(&self, inst_ref: InstRef) -> bool {
        self.func(inst_ref.func).insts.contains_key(inst_ref.inst)
    }

    fn inst_has_no_uses_alg(&self, inst_ref: InstRef) -> bool {
        self.value_uses(ValueId::Inst(inst_ref)).is_empty()
    }

    fn try_simplify_inst(&mut self, inst_ref: InstRef) -> Option<ValueId> {
        let inst = self.inst(inst_ref);
        let kind = inst.kind.clone();
        let ty = inst.ty.clone();

        match &kind {
            InstKind::Binary { op, lhs, rhs } => self.try_simplify_binary(op, *lhs, *rhs, &ty),
            InstKind::ICmp { op, lhs, rhs } => self.try_simplify_icmp(*op, *lhs, *rhs, &ty),
            InstKind::Select {
                cond,
                then_val,
                else_val,
            } => self.try_simplify_select(*cond, *then_val, *else_val),
            InstKind::Phi { incomings, .. } => self.try_simplify_phi(incomings),
            InstKind::Load { ptr } => self.try_fold_const_global_load(*ptr, &ty),
            InstKind::Trunc { value } => self.try_fold_trunc(*value, &ty),
            InstKind::Zext { value } => self.try_fold_zext(*value, &ty),
            InstKind::Sext { value } => self.try_fold_sext(*value, &ty),
            InstKind::PtrToInt { ptr } => self.try_fold_ptr_to_int(*ptr, &ty),
            _ => None,
        }
    }

    fn try_simplify_binary(
        &mut self,
        op: &BinaryOpcode,
        lhs: ValueId,
        rhs: ValueId,
        ty: &TypePtr,
    ) -> Option<ValueId> {
        let (lhs_const, rhs_const) = (self.as_int_const(lhs), self.as_int_const(rhs));

        if let (Some(a), Some(b)) = (lhs_const, rhs_const) {
            return self.fold_binary_const_alg(*op, a, b, ty);
        }

        match op {
            BinaryOpcode::Add => {
                if self.is_zero_const(rhs) {
                    return Some(lhs);
                }
                if self.is_zero_const(lhs) {
                    return Some(rhs);
                }
            }
            BinaryOpcode::Sub => {
                if self.is_zero_const(rhs) {
                    return Some(lhs);
                }
                if lhs == rhs {
                    return self.make_zero_const(ty);
                }
            }
            BinaryOpcode::Mul => {
                if self.is_one_const(rhs) {
                    return Some(lhs);
                }
                if self.is_one_const(lhs) {
                    return Some(rhs);
                }
                if self.is_zero_const(rhs) || self.is_zero_const(lhs) {
                    return self.make_zero_const(ty);
                }
            }
            BinaryOpcode::UDiv | BinaryOpcode::SDiv => {
                if self.is_one_const(rhs) {
                    return Some(lhs);
                }
                if self.is_zero_const(lhs) {
                    return self.make_zero_const(ty);
                }
            }
            BinaryOpcode::URem | BinaryOpcode::SRem => {
                if self.is_one_const(rhs) || self.is_zero_const(lhs) {
                    return self.make_zero_const(ty);
                }
            }
            BinaryOpcode::And => {
                if self.is_zero_const(rhs) || self.is_zero_const(lhs) {
                    return self.make_zero_const(ty);
                }
                if self.is_all_ones_const(rhs, ty) {
                    return Some(lhs);
                }
                if self.is_all_ones_const(lhs, ty) {
                    return Some(rhs);
                }
                if lhs == rhs {
                    return Some(lhs);
                }
            }
            BinaryOpcode::Or => {
                if self.is_zero_const(rhs) {
                    return Some(lhs);
                }
                if self.is_zero_const(lhs) {
                    return Some(rhs);
                }
                if self.is_all_ones_const(rhs, ty) || self.is_all_ones_const(lhs, ty) {
                    return self.make_all_ones_const(ty);
                }
                if lhs == rhs {
                    return Some(lhs);
                }
            }
            BinaryOpcode::Xor => {
                if self.is_zero_const(rhs) {
                    return Some(lhs);
                }
                if self.is_zero_const(lhs) {
                    return Some(rhs);
                }
                if lhs == rhs {
                    return self.make_zero_const(ty);
                }
            }
            BinaryOpcode::Shl | BinaryOpcode::LShr | BinaryOpcode::AShr => {
                if self.is_zero_const(rhs) {
                    return Some(lhs);
                }
                if self.is_zero_const(lhs) {
                    return self.make_zero_const(ty);
                }
            }
        }

        None
    }

    fn try_fold_const_global_load(&mut self, ptr: ValueId, ty: &TypePtr) -> Option<ValueId> {
        let ValueId::Global(global) = ptr else {
            return None;
        };

        let global_data = self.global(global);
        let GlobalKind::GlobalVariable {
            is_constant: true,
            initializer: Some(initializer),
        } = global_data.kind
        else {
            return None;
        };

        match &self.const_data(initializer).kind {
            ConstKind::Int(value) if ty.as_int().is_some() => Some(ValueId::Const(
                self.add_const(ty.clone(), ConstKind::Int(value.clone())),
            )),
            ConstKind::Null if ty.is_ptr() => {
                Some(ValueId::Const(self.add_const(ty.clone(), ConstKind::Null)))
            }
            _ => None,
        }
    }

    fn try_simplify_icmp(
        &mut self,
        op: ICmpCode,
        lhs: ValueId,
        rhs: ValueId,
        ty: &TypePtr,
    ) -> Option<ValueId> {
        let (lhs_const, rhs_const) = (self.as_int_const(lhs), self.as_int_const(rhs));

        if let (Some(a), Some(b)) = (lhs_const, rhs_const) {
            let result = match op {
                ICmpCode::Eq => a.cmp_eq(b),
                ICmpCode::Ne => a.cmp_ne(b),
                ICmpCode::Ugt => a.cmp_ugt(b),
                ICmpCode::Uge => a.cmp_uge(b),
                ICmpCode::Ult => a.cmp_ult(b),
                ICmpCode::Ule => a.cmp_ule(b),
                ICmpCode::Sgt => a.cmp_sgt(b),
                ICmpCode::Sge => a.cmp_sge(b),
                ICmpCode::Slt => a.cmp_slt(b),
                ICmpCode::Sle => a.cmp_sle(b),
            };
            return self.make_bool_const(result, ty);
        }

        if lhs == rhs {
            return match op {
                ICmpCode::Eq | ICmpCode::Uge | ICmpCode::Ule | ICmpCode::Sge | ICmpCode::Sle => {
                    self.make_bool_const(true, ty)
                }
                ICmpCode::Ne | ICmpCode::Ugt | ICmpCode::Ult | ICmpCode::Sgt | ICmpCode::Slt => {
                    self.make_bool_const(false, ty)
                }
            };
        }

        None
    }

    fn try_simplify_select(
        &self,
        cond: ValueId,
        then_val: ValueId,
        else_val: ValueId,
    ) -> Option<ValueId> {
        if then_val == else_val {
            return Some(then_val);
        }

        if let Some(c) = self.as_int_const(cond) {
            return if c.as_u64() != 0 {
                Some(then_val)
            } else {
                Some(else_val)
            };
        }

        None
    }

    fn try_simplify_phi(&self, incomings: &HashMap<usize, PhiIncoming>) -> Option<ValueId> {
        let mut values = incomings.values().map(|v| v.value);
        let first = values.next()?;
        for v in values {
            if v != first {
                return None;
            }
        }
        Some(first)
    }

    fn try_fold_trunc(&mut self, value: ValueId, ty: &TypePtr) -> Option<ValueId> {
        let const_val = self.as_int_const(value)?;
        let dst_bits = ty.as_int()?.0;
        if const_val.bit_width <= dst_bits {
            Some(value)
        } else {
            let truncated = const_val.trunc_to(dst_bits);
            Some(ValueId::Const(
                self.add_const(ty.clone(), ConstKind::Int(truncated)),
            ))
        }
    }

    fn try_fold_zext(&mut self, value: ValueId, ty: &TypePtr) -> Option<ValueId> {
        let const_val = self.as_int_const(value)?;
        let dst_bits = ty.as_int()?.0;
        if const_val.bit_width >= dst_bits {
            Some(value)
        } else {
            let extended = const_val.zero_extend(dst_bits);
            Some(ValueId::Const(
                self.add_const(ty.clone(), ConstKind::Int(extended)),
            ))
        }
    }

    fn try_fold_sext(&mut self, value: ValueId, ty: &TypePtr) -> Option<ValueId> {
        let const_val = self.as_int_const(value)?;
        let dst_bits = ty.as_int()?.0;
        if const_val.bit_width >= dst_bits {
            Some(value)
        } else {
            let extended = const_val.sign_extend(dst_bits);
            Some(ValueId::Const(
                self.add_const(ty.clone(), ConstKind::Int(extended)),
            ))
        }
    }

    fn try_fold_ptr_to_int(&mut self, value: ValueId, ty: &TypePtr) -> Option<ValueId> {
        if let ValueId::Const(const_id) = value
            && let ConstKind::Null = self.const_data(const_id).kind
        {
            return self.make_zero_const(ty);
        }
        None
    }

    fn as_int_const(&self, value: ValueId) -> Option<CoreInt> {
        match value {
            ValueId::Const(const_id) => match &self.const_data(const_id).kind {
                ConstKind::Int(c) => Some(c.clone()),
                ConstKind::Null => Some(CoreInt::new(0, 32)),
                ConstKind::Undef => Some(CoreInt::new(0, 1)),
                _ => None,
            },
            _ => None,
        }
    }

    fn is_zero_const(&self, value: ValueId) -> bool {
        match value {
            ValueId::Const(const_id) => match &self.const_data(const_id).kind {
                ConstKind::Int(c) => c.as_u64() == 0,
                ConstKind::Null => true,
                _ => false,
            },
            _ => false,
        }
    }

    fn is_one_const(&self, value: ValueId) -> bool {
        match value {
            ValueId::Const(const_id) => match &self.const_data(const_id).kind {
                ConstKind::Int(c) => c.as_u64() == 1,
                _ => false,
            },
            _ => false,
        }
    }

    fn is_all_ones_const(&self, value: ValueId, ty: &TypePtr) -> bool {
        match value {
            ValueId::Const(const_id) => match &self.const_data(const_id).kind {
                ConstKind::Int(c) => {
                    if let Some(int_ty) = ty.as_int() {
                        let mask = if int_ty.0 >= 64 {
                            u64::MAX
                        } else {
                            (1u64 << int_ty.0) - 1
                        };
                        c.as_u64() & mask == mask
                    } else {
                        false
                    }
                }
                _ => false,
            },
            _ => false,
        }
    }

    fn make_zero_const(&mut self, ty: &TypePtr) -> Option<ValueId> {
        if let Some(int_ty) = ty.as_int() {
            Some(ValueId::Const(self.add_const(
                ty.clone(),
                ConstKind::Int(CoreInt::new(0, int_ty.0)),
            )))
        } else {
            None
        }
    }

    fn make_all_ones_const(&mut self, ty: &TypePtr) -> Option<ValueId> {
        if let Some(int_ty) = ty.as_int() {
            let bits = int_ty.0;
            let value = if bits >= 64 {
                u64::MAX
            } else {
                (1u64 << bits) - 1
            };
            Some(ValueId::Const(self.add_const(
                ty.clone(),
                ConstKind::Int(CoreInt::new(value, bits)),
            )))
        } else {
            None
        }
    }

    fn make_bool_const(&mut self, value: bool, ty: &TypePtr) -> Option<ValueId> {
        if let Some(int_ty) = ty.as_int() {
            Some(ValueId::Const(self.add_const(
                ty.clone(),
                ConstKind::Int(CoreInt::new(if value { 1 } else { 0 }, int_ty.0)),
            )))
        } else {
            None
        }
    }

    fn fold_binary_const_alg(
        &mut self,
        op: BinaryOpcode,
        lhs: CoreInt,
        rhs: CoreInt,
        ty: &TypePtr,
    ) -> Option<ValueId> {
        let result = match op {
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
        }?;

        Some(ValueId::Const(
            self.add_const(ty.clone(), ConstKind::Int(result)),
        ))
    }
}
