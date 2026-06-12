use std::rc::Rc;

use crate::ir::{
    core::{FunctionId, InstRef, ModuleCore, ValueId},
    core_inst::InstKind,
    core_value::ConstKind,
    ir_type::{IntType, Type, TypePtr, VoidType},
};

impl ModuleCore {
    pub fn opt_scalar_memcpy(&mut self) {
        for id in self.functions_in_order() {
            self.func_scalar_memcpy(id);
        }
    }

    fn func_scalar_memcpy(&mut self, id: FunctionId) {
        let function = self.func(id);
        if function.is_declare {
            return;
        }

        let insts: Vec<_> = function
            .insts
            .keys()
            .map(|inst| InstRef { func: id, inst })
            .collect();

        for inst_ref in insts {
            if !self.func(id).insts.contains_key(inst_ref.inst) {
                continue;
            }
            if let Some((dest, src, ty)) = self.match_scalar_memcpy(inst_ref) {
                let load = self.new_inst(
                    id,
                    ty,
                    InstKind::Load { ptr: src },
                    self.inst(inst_ref).name.clone(),
                );
                let store = self.new_inst(
                    id,
                    Rc::new(Type::Void(VoidType)),
                    InstKind::Store {
                        value: ValueId::Inst(load),
                        ptr: dest,
                    },
                    None,
                );
                self.insert_before(inst_ref, load);
                self.insert_before(inst_ref, store);
                self.erase_inst_from_parent(inst_ref);
            }
        }
    }

    fn match_scalar_memcpy(&self, inst_ref: InstRef) -> Option<(ValueId, ValueId, TypePtr)> {
        let InstKind::Call { func, args } = &self.inst(inst_ref).kind else {
            return None;
        };
        let callee_name = &self.func(*func).name;
        if !(callee_name.starts_with("llvm.memcpy.") || callee_name.starts_with("llvm.memmove."))
            || args.len() < 4
        {
            return None;
        }

        if self.const_int_u64(args[3])? != 0 {
            return None;
        }

        let size = self.const_int_u64(args[2])?;
        let max_scalar_size = self
            .target_data_layout()
            .map(|layout| layout.pointer_size as u64)
            .unwrap_or(4);
        if size > max_scalar_size {
            return None;
        }

        let bits = match size {
            1 => 8,
            2 => 16,
            4 => 32,
            8 => 64,
            _ => return None,
        };

        Some((args[0], args[1], Rc::new(Type::Int(IntType(bits as u8)))))
    }

    fn const_int_u64(&self, value: ValueId) -> Option<u64> {
        let ValueId::Const(constant) = value else {
            return None;
        };
        let ConstKind::Int(number) = &self.const_data(constant).kind else {
            return None;
        };
        Some(number.as_u64())
    }
}
