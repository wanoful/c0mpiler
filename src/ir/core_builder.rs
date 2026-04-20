use crate::ir::{
    core::{BlockRef, FunctionId, InstRef, ModuleCore, ValueId},
    core_inst::{BinaryOpcode, CondBranch, ICmpCode, InstKind, PhiIncoming},
    ir_type::TypePtr,
};

#[derive(Debug, Clone, Copy)]
pub enum InsertPoint {
    AppendPhi(BlockRef),
    BeforeFirstNonPhi(BlockRef),
    BeforeTerminator(BlockRef),
    Before(InstRef),
    After(InstRef),
}

pub struct IRBuilder<'a> {
    pub module: &'a mut ModuleCore,
    pub insert_point: InsertPoint,
}

impl<'a> IRBuilder<'a> {
    pub fn new(module: &'a mut ModuleCore, insert_point: InsertPoint) -> Self {
        Self {
            module,
            insert_point,
        }
    }

    pub fn locate_append_phi(&mut self, block: BlockRef) {
        self.insert_point = InsertPoint::AppendPhi(block);
    }

    pub fn locate_before_first_non_phi(&mut self, block: BlockRef) {
        self.insert_point = InsertPoint::BeforeFirstNonPhi(block);
    }

    pub fn locate_before_terminator(&mut self, block: BlockRef) {
        self.insert_point = InsertPoint::BeforeTerminator(block);
    }

    pub fn locate_before(&mut self, inst: InstRef) {
        self.insert_point = InsertPoint::Before(inst);
    }

    pub fn locate_after(&mut self, inst: InstRef) {
        self.insert_point = InsertPoint::After(inst);
    }

    fn current_func(&self) -> FunctionId {
        match self.insert_point {
            InsertPoint::AppendPhi(block)
            | InsertPoint::BeforeFirstNonPhi(block)
            | InsertPoint::BeforeTerminator(block) => block.func,
            InsertPoint::Before(inst) | InsertPoint::After(inst) => inst.func,
        }
    }

    fn insert_inst(&mut self, inst: InstRef) {
        match self.insert_point {
            InsertPoint::AppendPhi(block_ref) => {
                self.module.append_phi(block_ref, inst);
            }
            InsertPoint::BeforeFirstNonPhi(block_ref) => {
                self.module.push_front_inst(block_ref, inst);
            }
            InsertPoint::BeforeTerminator(block_ref) => {
                if let Some(term) = self.module.block(block_ref).terminator {
                    self.module.insert_before(
                        InstRef {
                            func: block_ref.func,
                            inst: term,
                        },
                        inst,
                    );
                } else {
                    self.module.append_inst(block_ref, inst);
                }
            }
            InsertPoint::Before(inst_ref) => {
                self.module.insert_before(inst_ref, inst);
            }
            InsertPoint::After(inst_ref) => {
                self.module.insert_after(inst_ref, inst);
            }
        }
    }

    fn build_inst(&mut self, ty: TypePtr, kind: InstKind, name: Option<&str>) -> InstRef {
        let inst = self
            .module
            .new_inst(self.current_func(), ty, kind, name.map(str::to_string));
        self.insert_inst(inst);
        inst
    }

    pub fn append_block(&mut self, func: FunctionId, name: Option<&str>) -> BlockRef {
        self.module.append_block(func, name.map(str::to_string))
    }

    pub fn build_alloca(
        &mut self,
        ptr_ty: TypePtr,
        alloc_ty: TypePtr,
        name: Option<&str>,
    ) -> InstRef {
        self.build_inst(ptr_ty, InstKind::Alloca { ty: alloc_ty }, name)
    }

    pub fn build_load(&mut self, ty: TypePtr, ptr: ValueId, name: Option<&str>) -> InstRef {
        self.build_inst(ty, InstKind::Load { ptr }, name)
    }

    pub fn build_store(&mut self, void_ty: TypePtr, value: ValueId, ptr: ValueId) -> InstRef {
        self.build_inst(void_ty, InstKind::Store { value, ptr }, None)
    }

    pub fn build_binary(
        &mut self,
        op: BinaryOpcode,
        ty: TypePtr,
        lhs: ValueId,
        rhs: ValueId,
        name: Option<&str>,
    ) -> InstRef {
        self.build_inst(ty, InstKind::Binary { op, lhs, rhs }, name)
    }

    pub fn build_icmp(
        &mut self,
        op: ICmpCode,
        bool_ty: TypePtr,
        lhs: ValueId,
        rhs: ValueId,
        name: Option<&str>,
    ) -> InstRef {
        self.build_inst(bool_ty, InstKind::ICmp { op, lhs, rhs }, name)
    }

    pub fn build_getelementptr(
        &mut self,
        ptr_ty: TypePtr,
        base_ty: TypePtr,
        base: ValueId,
        indices: Vec<ValueId>,
        name: Option<&str>,
    ) -> InstRef {
        self.build_inst(
            ptr_ty,
            InstKind::GetElementPtr {
                base_ty,
                base,
                indices,
            },
            name,
        )
    }

    pub fn build_call(
        &mut self,
        func: FunctionId,
        args: Vec<ValueId>,
        name: Option<&str>,
    ) -> InstRef {
        let ret_ty = self.module.func(func).ty.0.as_function().unwrap().0.clone();
        self.build_inst(ret_ty, InstKind::Call { func, args }, name)
    }

    pub fn build_phi(
        &mut self,
        ty: TypePtr,
        incomings: Vec<(ValueId, BlockRef)>,
        name: Option<&str>,
    ) -> InstRef {
        let func = self.current_func();
        let incomings = incomings
            .into_iter()
            .map(|(value, block)| {
                assert_eq!(block.func, func);
                PhiIncoming {
                    block: block.block,
                    value,
                }
            })
            .collect();
        self.build_inst(ty, InstKind::Phi { incomings }, name)
    }

    pub fn build_select(
        &mut self,
        ty: TypePtr,
        cond: ValueId,
        then_val: ValueId,
        else_val: ValueId,
        name: Option<&str>,
    ) -> InstRef {
        self.build_inst(
            ty,
            InstKind::Select {
                cond,
                then_val,
                else_val,
            },
            name,
        )
    }

    pub fn build_ptr_to_int(
        &mut self,
        to_ty: TypePtr,
        ptr: ValueId,
        name: Option<&str>,
    ) -> InstRef {
        self.build_inst(to_ty, InstKind::PtrToInt { ptr }, name)
    }

    pub fn build_trunc(&mut self, to_ty: TypePtr, value: ValueId, name: Option<&str>) -> InstRef {
        self.build_inst(to_ty, InstKind::Trunc { value }, name)
    }

    pub fn build_zext(&mut self, to_ty: TypePtr, value: ValueId, name: Option<&str>) -> InstRef {
        self.build_inst(to_ty, InstKind::Zext { value }, name)
    }

    pub fn build_sext(&mut self, to_ty: TypePtr, value: ValueId, name: Option<&str>) -> InstRef {
        self.build_inst(to_ty, InstKind::Sext { value }, name)
    }

    pub fn build_branch(&mut self, void_ty: TypePtr, dest: BlockRef) -> InstRef {
        assert_eq!(self.current_func(), dest.func);
        self.build_inst(
            void_ty,
            InstKind::Branch {
                then_block: dest.block,
                cond: None,
            },
            None,
        )
    }

    pub fn build_conditional_branch(
        &mut self,
        void_ty: TypePtr,
        cond: ValueId,
        then_block: BlockRef,
        else_block: BlockRef,
    ) -> InstRef {
        let func = self.current_func();
        assert_eq!(func, then_block.func);
        assert_eq!(func, else_block.func);
        self.build_inst(
            void_ty,
            InstKind::Branch {
                then_block: then_block.block,
                cond: Some(CondBranch {
                    cond,
                    else_block: else_block.block,
                }),
            },
            None,
        )
    }

    pub fn build_return(&mut self, void_ty: TypePtr, value: Option<ValueId>) -> InstRef {
        self.build_inst(void_ty, InstKind::Ret { value }, None)
    }

    pub fn build_unreachable(&mut self, void_ty: TypePtr) -> InstRef {
        self.build_inst(void_ty, InstKind::Unreachable, None)
    }
}
