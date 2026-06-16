use std::collections::HashMap;

use crate::ir::{
    cfg::{CFGNode, DominatorTree},
    core::{FunctionId, InstRef, ModuleCore, ValueId},
    core_inst::{BinaryOpcode, ICmpCode, InstKind},
    core_value::ConstKind,
    ir_type::TypePtr,
};

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
    pub fn opt_global_gvn(&mut self) {
        for id in self.functions_in_order() {
            self.func_global_gvn(id);
        }
    }

    fn func_global_gvn(&mut self, id: FunctionId) {
        if self.func(id).is_declare {
            return;
        }

        let cfg = self.build_cfg(id);
        let dom_tree = cfg.build_dom_tree();

        // Walk dominator tree, inheriting value table from idom
        let mut to_remove = Vec::new();
        self.gvn_dom_walk(
            id,
            &dom_tree,
            CFGNode::Block(self.func(id).entry),
            &HashMap::new(),
            &mut to_remove,
        );

        for inst_ref in to_remove {
            if self.func(inst_ref.func).insts.contains_key(inst_ref.inst)
                && self.value_uses(ValueId::Inst(inst_ref)).is_empty()
            {
                self.erase_inst_from_parent(inst_ref);
            }
        }
    }

    fn gvn_dom_walk(
        &mut self,
        func_id: FunctionId,
        dom_tree: &DominatorTree,
        node: CFGNode,
        parent_table: &HashMap<ExprKey, ValueId>,
        to_remove: &mut Vec<InstRef>,
    ) {
        let Some(block_id) = node.as_block().copied() else {
            // Also walk children of Fake node
            for &child in dom_tree.children.get(&node).into_iter().flatten() {
                self.gvn_dom_walk(func_id, dom_tree, child, parent_table, to_remove);
            }
            return;
        };

        let block_ref = crate::ir::core::BlockRef {
            func: func_id,
            block: block_id,
        };

        // Inherit parent's value table
        let mut value_table: HashMap<ExprKey, ValueId> = parent_table.clone();

        // Process phi nodes first - they define new values
        for _phi_ref in self.phis_in_order(block_ref) {
            // Phi values are always new, just record them
            // (phis could be CSE'd but that's handled by algebraic simplification)
        }

        // Process instructions
        for inst_ref in self.insts_in_order(block_ref) {
            let kind = self.inst(inst_ref).kind.clone();
            let ty = self.inst(inst_ref).ty.clone();

            let key = match &kind {
                InstKind::Binary { op, lhs, rhs } => {
                    let lk = self.canon_gvn(*lhs);
                    let rk = self.canon_gvn(*rhs);
                    let (lk, rk) = match op {
                        BinaryOpcode::Add
                        | BinaryOpcode::Mul
                        | BinaryOpcode::And
                        | BinaryOpcode::Or
                        | BinaryOpcode::Xor => {
                            if cmp_canon_gvn(&lk, &rk) {
                                (lk, rk)
                            } else {
                                (rk, lk)
                            }
                        }
                        _ => (lk, rk),
                    };
                    Some(ExprKey::Binary(*op, lk, rk))
                }
                InstKind::ICmp { op, lhs, rhs } => {
                    let lk = self.canon_gvn(*lhs);
                    let rk = self.canon_gvn(*rhs);
                    let (op2, lk, rk) = match op {
                        ICmpCode::Eq | ICmpCode::Ne => {
                            if cmp_canon_gvn(&lk, &rk) {
                                (*op, lk, rk)
                            } else {
                                (*op, rk, lk)
                            }
                        }
                        _ => (*op, lk, rk),
                    };
                    Some(ExprKey::ICmp(op2, lk, rk))
                }
                InstKind::Trunc { value } => Some(ExprKey::Trunc(self.canon_gvn(*value), ty.clone())),
                InstKind::Zext { value } => Some(ExprKey::Zext(self.canon_gvn(*value), ty.clone())),
                InstKind::Sext { value } => Some(ExprKey::Sext(self.canon_gvn(*value), ty.clone())),
                InstKind::PtrToInt { ptr } => {
                    Some(ExprKey::PtrToInt(self.canon_gvn(*ptr), ty.clone()))
                }
                InstKind::GetElementPtr {
                    base_ty,
                    base,
                    indices,
                } => {
                    let base_k = self.canon_gvn(*base);
                    let idx_k = indices.iter().map(|v| self.canon_gvn(*v)).collect();
                    Some(ExprKey::Gep(base_ty.clone(), base_k, idx_k))
                }
                InstKind::Select {
                    cond,
                    then_val,
                    else_val,
                } => Some(ExprKey::Select(
                    self.canon_gvn(*cond),
                    self.canon_gvn(*then_val),
                    self.canon_gvn(*else_val),
                )),
                _ => None,
            };

            if let Some(k) = key {
                if let Some(&prev) = value_table.get(&k) {
                    // Found equivalent - replace uses and mark for removal
                    self.replace_all_uses_with(ValueId::Inst(inst_ref), prev);
                    to_remove.push(inst_ref);
                } else {
                    value_table.insert(k, ValueId::Inst(inst_ref));
                }
            }

            }

        // Recurse to children in dominator tree
        for &child in dom_tree
            .children
            .get(&node)
            .into_iter()
            .flatten()
        {
            self.gvn_dom_walk(func_id, dom_tree, child, &value_table, to_remove);
        }
    }

    fn canon_gvn(&self, value: ValueId) -> CanonValue {
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

fn cmp_canon_gvn(a: &CanonValue, b: &CanonValue) -> bool {
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
