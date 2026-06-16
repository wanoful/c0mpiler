use std::collections::HashMap;

use crate::ir::{
    core::{FunctionId, InstRef, ModuleCore, ValueId},
    core_inst::InstKind,
    core_value::ConstKind,
    ir_type::Type,
};

impl ModuleCore {
    pub fn opt_sroa(&mut self) {
        for id in self.functions_in_order() {
            self.func_sroa(id);
        }
    }

    fn func_sroa(&mut self, id: FunctionId) {
        if self.func(id).is_declare {
            return;
        }

        // Collect all allocas with struct/array type (snapshot to avoid borrow issues)
        let allocas: Vec<InstRef> = self
            .func(id)
            .insts
            .iter()
            .filter_map(|(inst_id, inst)| {
                if matches!(inst.kind, InstKind::Alloca { .. }) {
                    Some(InstRef {
                        func: id,
                        inst: inst_id,
                    })
                } else {
                    None
                }
            })
            .collect();

        for alloca_ref in allocas {
            if !self.func(id).insts.contains_key(alloca_ref.inst) {
                continue;
            }
            self.try_sroa_one(alloca_ref);
        }
    }

    fn try_sroa_one(&mut self, alloca_ref: InstRef) {
        let func_id = alloca_ref.func;
        let alloca_ty = self.inst(alloca_ref).ty.clone();

        // Get field types: struct or array
        let fields: Vec<crate::ir::ir_type::TypePtr> = match alloca_ty.as_ref() {
            Type::Struct(s) => s.get_body().unwrap_or_default(),
            Type::Array(a) => (0..a.1).map(|_| a.0.clone()).collect(),
            _ => return,
        };
        if fields.is_empty() {
            return;
        }

        // Collect all GEPs that use this alloca
        let gep_infos: Vec<(InstRef, u64)> = {
            let uses: Vec<_> = self.value_uses(ValueId::Inst(alloca_ref)).to_vec();
            let mut geps = Vec::new();
            for use_ in uses {
                let gep_ref = use_.user;
                let gep_inst = self.inst(gep_ref);
                if let InstKind::GetElementPtr { base, indices, .. } = &gep_inst.kind {
                    if *base == ValueId::Inst(alloca_ref) && indices.len() == 2 {
                        if let ValueId::Const(c) = indices[0] {
                            if let ConstKind::Int(ci) = &self.const_data(c).kind {
                                if ci.value == 0 {
                                    if let ValueId::Const(c2) = indices[1] {
                                        if let ConstKind::Int(ci2) = &self.const_data(c2).kind {
                                            geps.push((gep_ref, ci2.value));
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            geps
        };

        if gep_infos.is_empty() {
            return;
        }

        // Check that all GEP users are only Load or Store
        let mut valid = true;
        let mut field_accesses: HashMap<u64, Vec<(InstRef, crate::ir::core_inst::OperandSlot)>> =
            HashMap::new();

        for (gep_ref, field_idx) in &gep_infos {
            if *field_idx as usize >= fields.len() {
                valid = false;
                break;
            }
            let gep_users: Vec<_> = self.value_uses(ValueId::Inst(*gep_ref)).to_vec();
            for use_ in gep_users {
                let user_inst = self.inst(use_.user);
                match (&user_inst.kind, use_.slot) {
                    (
                        InstKind::Load { .. },
                        crate::ir::core_inst::OperandSlot::LoadPtr,
                    )
                    | (
                        InstKind::Store { .. },
                        crate::ir::core_inst::OperandSlot::StorePtr,
                    ) => {
                        field_accesses
                            .entry(*field_idx)
                            .or_default()
                            .push((use_.user, use_.slot));
                    }
                    _ => {
                        valid = false;
                    }
                }
            }
        }

        if !valid {
            return;
        }

        // Create field allocas
        let alloca_block = self.inst(alloca_ref).parent.unwrap();
        let mut field_allocas: HashMap<u64, InstRef> = HashMap::new();
        let ptr_ty = crate::ir::ir_type::TypePtr::new(Type::Ptr(
            crate::ir::ir_type::PtrType,
        ));

        for (&field_idx, _) in &field_accesses {
            let field_ty = fields[field_idx as usize].clone();
            let field_alloca = self.new_inst(
                func_id,
                ptr_ty.clone(),
                InstKind::Alloca { ty: field_ty },
                None,
            );
            self.append_inst(alloca_block, field_alloca);
            field_allocas.insert(field_idx, field_alloca);
        }

        // Rewrite Load/Store to use field allocas
        for (field_idx, accesses) in &field_accesses {
            let field_alloca = field_allocas[field_idx];

            for (user_ref, slot) in accesses {
                let user_kind = self.inst(*user_ref).kind.clone();
                let user_ty = self.inst(*user_ref).ty.clone();
                let user_parent = self.inst(*user_ref).parent.unwrap();

                match (&user_kind, *slot) {
                    (InstKind::Load { .. }, crate::ir::core_inst::OperandSlot::LoadPtr) => {
                        let new_load = self.new_inst(
                            func_id,
                            user_ty,
                            InstKind::Load {
                                ptr: ValueId::Inst(field_alloca),
                            },
                            None,
                        );
                        self.detach_inst(*user_ref);
                        self.append_inst(user_parent, new_load);
                        self.replace_all_uses_with(
                            ValueId::Inst(*user_ref),
                            ValueId::Inst(new_load),
                        );
                        self.append_inst(user_parent, *user_ref);
                    }
                    (InstKind::Store { value, .. }, crate::ir::core_inst::OperandSlot::StorePtr) => {
                        let new_store = self.new_inst(
                            func_id,
                            std::rc::Rc::new(Type::Void(crate::ir::ir_type::VoidType)),
                            InstKind::Store {
                                value: *value,
                                ptr: ValueId::Inst(field_alloca),
                            },
                            None,
                        );
                        self.append_inst(user_parent, new_store);
                    }
                    _ => {}
                }
            }
        }
    }
}
