use std::collections::HashMap;

use crate::ir::{
    core::{BlockRef, FunctionId, ModuleCore, ValueId},
    core_inst::{BinaryOpcode, ICmpCode, InstKind},
    core_value::ConstKind,
    ir_type::TypePtr,
};

/// Canonical value used for hashing: collapses different ConstIds that hold
/// the same integer into a single key, so that local CSE can deduplicate
/// instructions whose only difference is which copy of an integer constant
/// they reference. Non-const values are keyed by their ValueId.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum CanonValue {
    Const(u8, u64),
    Null,
    Undef(TypePtr),
    Value(ValueId),
}

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum ExprKey {
    Binary(BinaryOpcode, CanonValue, CanonValue),
    ICmp(ICmpCode, CanonValue, CanonValue),
    Trunc(CanonValue, TypePtr),
    Zext(CanonValue, TypePtr),
    Sext(CanonValue, TypePtr),
    PtrToInt(CanonValue, TypePtr),
    Gep(TypePtr, CanonValue, Vec<CanonValue>),
    Select(CanonValue, CanonValue, CanonValue),
}

impl ModuleCore {
    pub fn opt_local_gvn(&mut self) {
        for id in self.functions_in_order() {
            self.func_local_gvn(id);
        }
    }

    fn func_local_gvn(&mut self, id: FunctionId) {
        if self.func(id).is_declare {
            return;
        }

        for block in self.blocks_in_order(id) {
            self.block_local_cse(block);
        }
    }

    fn block_local_cse(&mut self, block: BlockRef) {
        let mut seen: HashMap<ExprKey, ValueId> = HashMap::new();
        let mut to_remove = Vec::new();

        for inst_ref in self.insts_in_order(block) {
            let kind = self.inst(inst_ref).kind.clone();
            let ty = self.inst(inst_ref).ty.clone();

            let key = match &kind {
                InstKind::Binary { op, lhs, rhs } => {
                    let lk = self.canon(*lhs);
                    let rk = self.canon(*rhs);
                    let (lk, rk) = match op {
                        BinaryOpcode::Add
                        | BinaryOpcode::Mul
                        | BinaryOpcode::And
                        | BinaryOpcode::Or
                        | BinaryOpcode::Xor => {
                            // canonical commutative order
                            if cmp_canon(&lk, &rk) {
                                (lk, rk)
                            } else {
                                (rk, lk)
                            }
                        }
                        _ => (lk, rk),
                    };
                    ExprKey::Binary(*op, lk, rk)
                }
                InstKind::ICmp { op, lhs, rhs } => {
                    let lk = self.canon(*lhs);
                    let rk = self.canon(*rhs);
                    let (op2, lk, rk) = match op {
                        ICmpCode::Eq | ICmpCode::Ne => {
                            if cmp_canon(&lk, &rk) {
                                (*op, lk, rk)
                            } else {
                                (*op, rk, lk)
                            }
                        }
                        _ => (*op, lk, rk),
                    };
                    ExprKey::ICmp(op2, lk, rk)
                }
                InstKind::Trunc { value } => ExprKey::Trunc(self.canon(*value), ty.clone()),
                InstKind::Zext { value } => ExprKey::Zext(self.canon(*value), ty.clone()),
                InstKind::Sext { value } => ExprKey::Sext(self.canon(*value), ty.clone()),
                InstKind::PtrToInt { ptr } => ExprKey::PtrToInt(self.canon(*ptr), ty.clone()),
                InstKind::GetElementPtr {
                    base_ty,
                    base,
                    indices,
                } => {
                    let base_k = self.canon(*base);
                    let idx_k = indices.iter().map(|v| self.canon(*v)).collect();
                    ExprKey::Gep(base_ty.clone(), base_k, idx_k)
                }
                InstKind::Select {
                    cond,
                    then_val,
                    else_val,
                } => ExprKey::Select(
                    self.canon(*cond),
                    self.canon(*then_val),
                    self.canon(*else_val),
                ),
                _ => continue,
            };

            if let Some(&prev) = seen.get(&key) {
                self.replace_all_uses_with(ValueId::Inst(inst_ref), prev);
                to_remove.push(inst_ref);
            } else {
                seen.insert(key, ValueId::Inst(inst_ref));
            }
        }

        for inst_ref in to_remove {
            if self.func(inst_ref.func).insts.contains_key(inst_ref.inst)
                && self.value_uses(ValueId::Inst(inst_ref)).is_empty()
            {
                self.erase_inst_from_parent(inst_ref);
            }
        }
    }

    fn canon(&self, value: ValueId) -> CanonValue {
        if let ValueId::Const(const_id) = value {
            let data = self.const_data(const_id);
            match &data.kind {
                ConstKind::Int(ci) => return CanonValue::Const(ci.bit_width, ci.value),
                ConstKind::Null => return CanonValue::Null,
                ConstKind::Undef => return CanonValue::Undef(data.ty.clone()),
                _ => {}
            }
        }
        CanonValue::Value(value)
    }
}

fn cmp_canon(a: &CanonValue, b: &CanonValue) -> bool {
    // Stable ordering based on discriminant + content; returns true iff a <= b.
    use CanonValue::*;
    let ka = match a {
        Const(_, _) => 0,
        Null => 1,
        Undef(_) => 2,
        Value(_) => 3,
    };
    let kb = match b {
        Const(_, _) => 0,
        Null => 1,
        Undef(_) => 2,
        Value(_) => 3,
    };
    if ka != kb {
        return ka <= kb;
    }
    match (a, b) {
        (Const(w1, v1), Const(w2, v2)) => (w1, v1) <= (w2, v2),
        (Value(v1), Value(v2)) => format!("{:?}", v1) <= format!("{:?}", v2),
        _ => true,
    }
}
