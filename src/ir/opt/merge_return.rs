use std::rc::Rc;

use crate::ir::{
    core::{FunctionId, InstRef, ModuleCore, ValueId},
    core_inst::{InstKind, PhiIncoming},
    ir_type::{Type, VoidType},
};

impl ModuleCore {
    pub fn opt_merge_return(&mut self) {
        for id in self.functions_in_order() {
            self.func_merge_return(id);
        }
    }

    fn func_merge_return(&mut self, id: FunctionId) {
        let function = self.func(id);
        if function.is_declare {
            return;
        }

        let return_insts = function
            .insts
            .iter()
            .filter_map(|(inst_id, data)| {
                if let InstKind::Ret { .. } = data.kind {
                    Some(inst_id)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        if return_insts.len() <= 1 {
            return;
        }

        let new_block = self.append_block(id, Some(".ret".to_string()));
        let function = self.func(id);

        let ty = function.ty.return_type.clone();

        if !ty.is_void() {
            let incomings = return_insts
                .iter()
                .map(|&inst_id| {
                    let ret_inst = &function.insts[inst_id];
                    if let InstKind::Ret { value: Some(v) } = &ret_inst.kind {
                        PhiIncoming {
                            block: ret_inst.parent.unwrap().block,
                            value: *v,
                        }
                    } else {
                        panic!("Return instruction has no return value!  But the function return type is {:?}\n And the function type is {:?}", ty, function.ty);
                    }
                })
                .collect::<Vec<_>>();
            let phi = self.new_inst(id, ty, InstKind::Phi { incomings }, None);
            self.append_phi(new_block, phi);
            let ret = self.new_inst(
                id,
                Rc::new(Type::Void(VoidType)),
                InstKind::Ret {
                    value: Some(ValueId::Inst(phi)),
                },
                None,
            );
            self.set_terminator(new_block, ret);
        } else {
            let ret = self.new_inst(
                id,
                Rc::new(Type::Void(VoidType)),
                InstKind::Ret { value: None },
                None,
            );
            self.set_terminator(new_block, ret);
        }

        for inst_id in return_insts {
            let jal = self.new_inst(
                id,
                Rc::new(Type::Void(VoidType)),
                InstKind::Branch {
                    then_block: new_block.block,
                    cond: None,
                },
                None,
            );
            self.overwrite_inst(
                InstRef {
                    func: id,
                    inst: inst_id,
                },
                jal,
            );
        }
    }
}
