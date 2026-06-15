use std::collections::HashMap;

use crate::ir::{
    core::{BlockRef, FunctionId, InstRef, ModuleCore, ValueId},
    core_inst::InstKind,
    core_value::ConstKind,
    ir_type::TypePtr,
};

#[derive(Clone, PartialEq, Eq, Hash)]
enum CanonIndex {
    Const(u8, u64),
    Value(ValueId),
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct GepKey {
    base_ty: TypePtr,
    base: CanonIndex,
    indices: Vec<CanonIndex>,
}

impl ModuleCore {
    pub fn opt_local_memory(&mut self) {
        for id in self.functions_in_order() {
            self.func_local_memory(id);
        }
    }

    fn func_local_memory(&mut self, id: FunctionId) {
        if self.func(id).is_declare {
            return;
        }

        for block in self.blocks_in_order(id) {
            self.local_common_gep(block);
            self.local_forward_memory(block);
        }
    }

    fn local_common_gep(&mut self, block: BlockRef) {
        let mut seen: HashMap<GepKey, ValueId> = HashMap::new();
        let mut to_remove = Vec::new();

        for inst_ref in self.insts_in_order(block) {
            let kind = self.inst(inst_ref).kind.clone();
            if let InstKind::GetElementPtr {
                base_ty,
                base,
                indices,
            } = kind
            {
                let key = GepKey {
                    base_ty,
                    base: self.canon_index(base),
                    indices: indices.iter().map(|v| self.canon_index(*v)).collect(),
                };
                if let Some(&prev) = seen.get(&key) {
                    self.replace_all_uses_with(ValueId::Inst(inst_ref), prev);
                    to_remove.push(inst_ref);
                } else {
                    seen.insert(key, ValueId::Inst(inst_ref));
                }
            }
        }

        for inst_ref in to_remove {
            if self.value_uses(ValueId::Inst(inst_ref)).is_empty() {
                self.erase_inst_from_parent(inst_ref);
            }
        }
    }

    fn canon_index(&self, value: ValueId) -> CanonIndex {
        if let ValueId::Const(const_id) = value
            && let ConstKind::Int(ci) = &self.const_data(const_id).kind
        {
            return CanonIndex::Const(ci.bit_width, ci.value);
        }
        CanonIndex::Value(value)
    }

    fn local_forward_memory(&mut self, block: BlockRef) {
        let mut last_store: HashMap<ValueId, (InstRef, ValueId, TypePtr)> = HashMap::new();
        let mut last_load: HashMap<ValueId, (ValueId, TypePtr)> = HashMap::new();
        let mut to_remove = Vec::new();

        for inst_ref in self.insts_in_order(block) {
            let kind = self.inst(inst_ref).kind.clone();
            match kind {
                InstKind::Load { ptr } => {
                    if let Some((value, ty)) = last_store
                        .get(&ptr)
                        .map(|(_, value, ty)| (*value, ty.clone()))
                        .or_else(|| last_load.get(&ptr).cloned())
                        && self.value_ty(value) == &ty
                    {
                        self.replace_all_uses_with(ValueId::Inst(inst_ref), value);
                        to_remove.push(inst_ref);
                    } else {
                        let ty = self.inst(inst_ref).ty.clone();
                        self.kill_aliasing_stores(&mut last_store, ptr);
                        last_load.insert(ptr, (ValueId::Inst(inst_ref), ty));
                    }
                }
                InstKind::Store { value, ptr } => {
                    let stored_ty = self.value_ty(value).clone();
                    if let Some((prev_store, _, prev_ty)) = last_store
                        .get(&ptr)
                        .map(|(inst, value, ty)| (*inst, *value, ty.clone()))
                    {
                        if prev_ty == stored_ty {
                            to_remove.push(prev_store);
                        }
                    }

                    self.kill_aliasing_stores(&mut last_store, ptr);
                    self.kill_aliasing_loads(&mut last_load, ptr);
                    last_store.insert(ptr, (inst_ref, value, self.value_ty(value).clone()));
                    last_load.insert(ptr, (value, self.value_ty(value).clone()));
                }
                InstKind::Call { .. } => {
                    last_store.clear();
                    last_load.clear();
                }
                _ => {}
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

    fn kill_aliasing_stores(
        &self,
        values: &mut HashMap<ValueId, (InstRef, ValueId, TypePtr)>,
        ptr: ValueId,
    ) {
        let to_remove = values
            .keys()
            .copied()
            .filter(|&known_ptr| known_ptr != ptr && self.ptr_may_alias(known_ptr, ptr))
            .collect::<Vec<_>>();
        for known_ptr in to_remove {
            values.remove(&known_ptr);
        }
    }

    fn kill_aliasing_loads(&self, values: &mut HashMap<ValueId, (ValueId, TypePtr)>, ptr: ValueId) {
        let to_remove = values
            .keys()
            .copied()
            .filter(|&known_ptr| known_ptr != ptr && self.ptr_may_alias(known_ptr, ptr))
            .collect::<Vec<_>>();
        for known_ptr in to_remove {
            values.remove(&known_ptr);
        }
    }

    fn ptr_may_alias(&self, lhs: ValueId, rhs: ValueId) -> bool {
        if lhs == rhs {
            return true;
        }

        let Some((lhs_root, lhs_path)) = self.ptr_path(lhs) else {
            return true;
        };
        let Some((rhs_root, rhs_path)) = self.ptr_path(rhs) else {
            return true;
        };
        if lhs_root != rhs_root {
            return !self.roots_known_distinct_local(lhs_root, rhs_root);
        }

        for (lhs_index, rhs_index) in lhs_path.iter().zip(rhs_path.iter()) {
            if lhs_index == rhs_index {
                continue;
            }
            if self.const_indices_equal(*lhs_index, *rhs_index) {
                continue;
            }
            if self.const_indices_known_distinct(*lhs_index, *rhs_index) {
                return false;
            }
            return true;
        }

        true
    }

    fn roots_known_distinct_local(&self, lhs: ValueId, rhs: ValueId) -> bool {
        let is_root = |v: ValueId| {
            matches!(v, ValueId::Arg(..) | ValueId::Global(..))
                || matches!(v, ValueId::Inst(i) if matches!(self.inst(i).kind, InstKind::Alloca { .. }))
        };
        is_root(lhs) && is_root(rhs)
    }

    fn ptr_path(&self, ptr: ValueId) -> Option<(ValueId, Vec<ValueId>)> {
        let ValueId::Inst(inst_ref) = ptr else {
            return Some((ptr, Vec::new()));
        };
        let InstKind::GetElementPtr { base, indices, .. } = &self.inst(inst_ref).kind else {
            return Some((ptr, Vec::new()));
        };

        let (root, mut path) = self.ptr_path(*base)?;
        path.extend(indices.iter().copied());
        Some((root, path))
    }

    fn const_indices_known_distinct(&self, lhs: ValueId, rhs: ValueId) -> bool {
        let (ValueId::Const(lhs), ValueId::Const(rhs)) = (lhs, rhs) else {
            return false;
        };
        let ConstKind::Int(lhs) = &self.const_data(lhs).kind else {
            return false;
        };
        let ConstKind::Int(rhs) = &self.const_data(rhs).kind else {
            return false;
        };
        lhs != rhs
    }

    fn const_indices_equal(&self, lhs: ValueId, rhs: ValueId) -> bool {
        let (ValueId::Const(lhs), ValueId::Const(rhs)) = (lhs, rhs) else {
            return false;
        };
        let ConstKind::Int(lhs) = &self.const_data(lhs).kind else {
            return false;
        };
        let ConstKind::Int(rhs) = &self.const_data(rhs).kind else {
            return false;
        };
        lhs == rhs
    }
}
