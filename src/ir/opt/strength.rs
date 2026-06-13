use std::collections::HashMap;
use std::rc::Rc;

use crate::ir::{
    core::{FunctionId, InstRef, ModuleCore, ValueId},
    core_inst::{BinaryOpcode, InstKind},
    core_int::CoreInt,
    core_value::ConstKind,
    ir_type::{IntType, Type, TypePtr},
};

impl ModuleCore {
    /// IR-level strength reduction:
    /// - `mul x, 2^n` → `shl x, n`
    /// - `udiv x, 2^n` → `lshr x, n`
    /// - `urem x, 2^n` → `and x, 2^n - 1`
    /// - `sdiv x, 2^n` → `lshr x, n` when x is known non-negative
    /// - `srem x, 2^n` → `and x, 2^n - 1` when x is known non-negative
    pub fn opt_strength_reduction(&mut self) {
        for id in self.functions_in_order() {
            self.func_strength_reduction(id);
        }
    }

    fn func_strength_reduction(&mut self, id: FunctionId) {
        if self.func(id).is_declare {
            return;
        }

        // Cache non-negative analysis results within this function.
        let mut nonneg_cache: HashMap<ValueId, bool> = HashMap::new();

        let insts: Vec<InstRef> = self
            .func(id)
            .insts
            .keys()
            .map(|inst| InstRef { func: id, inst })
            .collect();

        for inst_ref in insts {
            if !self.func(id).insts.contains_key(inst_ref.inst) {
                continue;
            }

            let (op, lhs, rhs) = match &self.inst(inst_ref).kind {
                InstKind::Binary { op, lhs, rhs } => (*op, *lhs, *rhs),
                _ => continue,
            };

            let Some(rhs_const) = self.as_int_const_sr(rhs) else {
                continue;
            };
            let ty = self.inst(inst_ref).ty.clone();
            let Some(int_ty) = ty.as_int() else {
                continue;
            };
            let bits = int_ty.0;
            let rhs_val = rhs_const.value;

            // We require the constant to be a positive power of two within the bit range.
            if rhs_val == 0 {
                continue;
            }
            if !rhs_val.is_power_of_two() {
                continue;
            }
            let mask_for_bits = if bits >= 64 { u64::MAX } else { (1u64 << bits) - 1 };
            if rhs_val >= mask_for_bits.wrapping_add(1) >> 1 && bits < 64 {
                // rhs would be interpreted as negative — skip.
                continue;
            }

            let shift = rhs_val.trailing_zeros() as u64;

            let new_kind = match op {
                BinaryOpcode::Mul => {
                    let shift_const = self.add_const(
                        ty.clone(),
                        ConstKind::Int(CoreInt::new(shift, bits)),
                    );
                    InstKind::Binary {
                        op: BinaryOpcode::Shl,
                        lhs,
                        rhs: ValueId::Const(shift_const),
                    }
                }
                BinaryOpcode::UDiv => {
                    let shift_const = self.add_const(
                        ty.clone(),
                        ConstKind::Int(CoreInt::new(shift, bits)),
                    );
                    InstKind::Binary {
                        op: BinaryOpcode::LShr,
                        lhs,
                        rhs: ValueId::Const(shift_const),
                    }
                }
                BinaryOpcode::URem => {
                    let mask_const = self.add_const(
                        ty.clone(),
                        ConstKind::Int(CoreInt::new(rhs_val - 1, bits)),
                    );
                    InstKind::Binary {
                        op: BinaryOpcode::And,
                        lhs,
                        rhs: ValueId::Const(mask_const),
                    }
                }
                BinaryOpcode::SDiv if self.is_value_nonneg(lhs, &mut nonneg_cache) => {
                    let shift_const = self.add_const(
                        ty.clone(),
                        ConstKind::Int(CoreInt::new(shift, bits)),
                    );
                    InstKind::Binary {
                        op: BinaryOpcode::LShr,
                        lhs,
                        rhs: ValueId::Const(shift_const),
                    }
                }
                BinaryOpcode::SRem if self.is_value_nonneg(lhs, &mut nonneg_cache) => {
                    let mask_const = self.add_const(
                        ty.clone(),
                        ConstKind::Int(CoreInt::new(rhs_val - 1, bits)),
                    );
                    InstKind::Binary {
                        op: BinaryOpcode::And,
                        lhs,
                        rhs: ValueId::Const(mask_const),
                    }
                }
                _ => continue,
            };

            self.replace_inst_kind(inst_ref, new_kind);
            // The result now becomes non-negative for and/lshr/shl ops, so clear
            // cached values that might depend on this — simplest: clear cache.
            nonneg_cache.clear();
        }
    }

    fn replace_inst_kind(&mut self, inst_ref: InstRef, new_kind: InstKind) {
        // Manually rewrite use lists: remove old uses, install new kind, then
        // re-register uses for the new operands.
        self.unregister_inst_use_pub(inst_ref);
        self.inst_mut(inst_ref).kind = new_kind;
        self.register_inst_use_pub(inst_ref);
    }

    fn as_int_const_sr(&self, value: ValueId) -> Option<CoreInt> {
        let ValueId::Const(const_id) = value else {
            return None;
        };
        let ConstKind::Int(c) = &self.const_data(const_id).kind else {
            return None;
        };
        Some(c.clone())
    }

    /// Conservative non-negativity analysis. Returns true if we can prove that
    /// `value`, interpreted as a signed integer of its declared bit-width, is
    /// always >= 0.
    fn is_value_nonneg(&self, value: ValueId, cache: &mut HashMap<ValueId, bool>) -> bool {
        if let Some(&v) = cache.get(&value) {
            return v;
        }
        // Insert a placeholder to break cycles (phi/recursion). Pessimistic by default.
        cache.insert(value, false);
        let result = self.compute_nonneg(value, cache);
        cache.insert(value, result);
        result
    }

    fn compute_nonneg(&self, value: ValueId, cache: &mut HashMap<ValueId, bool>) -> bool {
        match value {
            ValueId::Const(c) => match &self.const_data(c).kind {
                ConstKind::Int(ci) => {
                    // Non-negative iff the sign bit of its declared bit width is 0.
                    if ci.bit_width == 0 {
                        return true;
                    }
                    let sign_bit = 1u64 << (ci.bit_width - 1);
                    (ci.value & sign_bit) == 0
                }
                _ => false,
            },
            ValueId::Arg(_) | ValueId::Global(_) => false,
            ValueId::Inst(inst_ref) => match &self.inst(inst_ref).kind {
                InstKind::Binary { op, lhs, rhs } => match op {
                    BinaryOpcode::And => {
                        self.is_value_nonneg(*lhs, cache) || self.is_value_nonneg(*rhs, cache)
                    }
                    BinaryOpcode::Or | BinaryOpcode::Xor => {
                        self.is_value_nonneg(*lhs, cache) && self.is_value_nonneg(*rhs, cache)
                    }
                    BinaryOpcode::LShr => true,
                    BinaryOpcode::UDiv | BinaryOpcode::URem => true,
                    BinaryOpcode::Shl
                    | BinaryOpcode::AShr
                    | BinaryOpcode::Add
                    | BinaryOpcode::Mul => {
                        self.is_value_nonneg(*lhs, cache) && self.is_value_nonneg(*rhs, cache)
                    }
                    BinaryOpcode::Sub => false,
                    BinaryOpcode::SDiv => {
                        self.is_value_nonneg(*lhs, cache) && self.is_value_nonneg(*rhs, cache)
                    }
                    BinaryOpcode::SRem => self.is_value_nonneg(*lhs, cache),
                },
                InstKind::Zext { .. } => true,
                InstKind::ICmp { .. } => true,
                InstKind::Phi { incomings, .. } => incomings
                    .values()
                    .all(|inc| self.is_value_nonneg(inc.value, cache)),
                InstKind::Select {
                    then_val,
                    else_val,
                    ..
                } => {
                    self.is_value_nonneg(*then_val, cache)
                        && self.is_value_nonneg(*else_val, cache)
                }
                _ => false,
            },
        }
    }
}

// Expose register/unregister methods on ModuleCore via a small trait shim. The
// originals are private — replicate the bodies inline.
impl ModuleCore {
    fn register_inst_use_pub(&mut self, user: InstRef) {
        let kind = self.inst(user).kind.clone();
        kind.for_each_value_operand(|value, slot| {
            self.value_uses_mut(value).push(crate::ir::core::Use { user, slot });
        });
    }

    fn unregister_inst_use_pub(&mut self, user: InstRef) {
        let kind = self.inst(user).kind.clone();
        kind.for_each_value_operand(|value, slot| {
            let uses = self.value_uses_mut(value);
            if let Some(pos) = uses.iter().position(|u| u.user == user && u.slot == slot) {
                uses.remove(pos);
            }
        });
    }
}

// Force unused-import linter to keep `Rc`, `IntType`, `Type`, `TypePtr` if they
// were referenced; the simplification above doesn't actually need them.
#[allow(dead_code)]
fn _force_imports() -> (Rc<Type>, IntType, TypePtr) {
    use crate::ir::ir_type::PtrType;
    let t = Rc::new(Type::Ptr(PtrType));
    (t.clone(), IntType(32), t)
}
