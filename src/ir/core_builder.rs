use std::{cell::RefCell, rc::Rc};

use crate::ir::{
    core::{BlockRef, FunctionId, InstRef, ModuleCore, ValueId},
    core_inst::{BinaryOpcode, CondBranch, ICmpCode, InstKind, PhiIncoming},
    ir_type::{FunctionType, IntType, PtrType, Type, TypePtr, VoidType},
};

#[derive(Debug, Clone, Copy)]
pub enum InsertPoint {
    AppendPhi(BlockRef),
    BeforeFirstNonPhi(BlockRef),
    BeforeTerminator(BlockRef),
    Before(InstRef),
    After(InstRef),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuilderInsertPlace {
    Front,
    End,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuilderLocation {
    pub func: FunctionId,
    pub block: BlockRef,
    pub insert_place: BuilderInsertPlace,
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

pub struct CursorBuilder {
    module: Rc<RefCell<ModuleCore>>,
    location: Option<BuilderLocation>,
}

impl CursorBuilder {
    pub fn new(module: Rc<RefCell<ModuleCore>>) -> Self {
        Self {
            module,
            location: None,
        }
    }

    pub fn module(&self) -> Rc<RefCell<ModuleCore>> {
        self.module.clone()
    }

    pub fn get_location(&self) -> Option<BuilderLocation> {
        self.location
    }

    pub fn set_location(&mut self, location: Option<BuilderLocation>) {
        self.location = location;
    }

    pub fn get_current_function(&self) -> FunctionId {
        self.location
            .as_ref()
            .expect("builder does not have a current insertion location")
            .func
    }

    pub fn get_current_basic_block(&self) -> BlockRef {
        self.location
            .as_ref()
            .expect("builder does not have a current insertion location")
            .block
    }

    pub fn locate_end(&mut self, func: FunctionId, block: BlockRef) {
        assert_eq!(func, block.func);
        self.location = Some(BuilderLocation {
            func,
            block,
            insert_place: BuilderInsertPlace::End,
        });
    }

    pub fn locate_front(&mut self, func: FunctionId, block: BlockRef) {
        assert_eq!(func, block.func);
        self.location = Some(BuilderLocation {
            func,
            block,
            insert_place: BuilderInsertPlace::Front,
        });
    }

    fn location(&self) -> BuilderLocation {
        self.location
            .expect("builder does not have a current insertion location")
    }

    fn void_ty() -> TypePtr {
        Rc::new(Type::Void(VoidType))
    }

    fn i1_ty() -> TypePtr {
        Rc::new(Type::Int(IntType(1)))
    }

    fn i32_ty() -> TypePtr {
        Rc::new(Type::Int(IntType(32)))
    }

    fn ptr_ty() -> TypePtr {
        Rc::new(Type::Ptr(PtrType))
    }

    fn memcpy_ty() -> TypePtr {
        Rc::new(Type::Function(FunctionType(
            Self::void_ty(),
            vec![
                Self::ptr_ty(),
                Self::ptr_ty(),
                Self::i32_ty(),
                Self::i1_ty(),
            ],
        )))
    }

    fn insert_inst(&mut self, inst: InstRef) {
        let location = self.location();
        let mut module = self.module.borrow_mut();
        let kind = module.inst(inst).kind.clone();

        match location.insert_place {
            BuilderInsertPlace::Front => {
                assert!(
                    !kind.is_terminator(),
                    "cannot insert a terminator at the front of a block"
                );
                if kind.is_phi() {
                    module.append_phi(location.block, inst);
                } else {
                    module.push_front_inst(location.block, inst);
                }
            }
            BuilderInsertPlace::End => {
                if kind.is_phi() {
                    module.append_phi(location.block, inst);
                } else if kind.is_terminator() {
                    module.set_terminator(location.block, inst);
                } else {
                    module.append_inst(location.block, inst);
                }
            }
        }
    }

    fn build_inst(&mut self, ty: TypePtr, kind: InstKind, name: Option<&str>) -> InstRef {
        let location = self.location();
        let inst = {
            let mut module = self.module.borrow_mut();
            module.new_inst(location.func, ty, kind, name.map(str::to_string))
        };
        self.insert_inst(inst);
        inst
    }

    pub fn append_block(&mut self, func: FunctionId, name: Option<&str>) -> BlockRef {
        self.module
            .borrow_mut()
            .append_block(func, name.map(str::to_string))
    }

    pub fn build_alloca(&mut self, alloc_ty: TypePtr, name: Option<&str>) -> InstRef {
        self.build_inst(Self::ptr_ty(), InstKind::Alloca { ty: alloc_ty }, name)
    }

    pub fn build_load(&mut self, ty: TypePtr, ptr: ValueId, name: Option<&str>) -> InstRef {
        self.build_inst(ty, InstKind::Load { ptr }, name)
    }

    pub fn build_store(&mut self, value: ValueId, ptr: ValueId) -> InstRef {
        self.build_inst(Self::void_ty(), InstKind::Store { value, ptr }, None)
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
        lhs: ValueId,
        rhs: ValueId,
        name: Option<&str>,
    ) -> InstRef {
        self.build_inst(Self::i1_ty(), InstKind::ICmp { op, lhs, rhs }, name)
    }

    pub fn build_getelementptr(
        &mut self,
        base_ty: TypePtr,
        base: ValueId,
        indices: Vec<ValueId>,
        name: Option<&str>,
    ) -> InstRef {
        self.build_inst(
            Self::ptr_ty(),
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
        let ret_ty = self
            .module
            .borrow()
            .func(func)
            .ty
            .0
            .as_function()
            .unwrap()
            .0
            .clone();
        self.build_inst(ret_ty, InstKind::Call { func, args }, name)
    }

    pub fn build_call_value(
        &mut self,
        callee: ValueId,
        args: Vec<ValueId>,
        name: Option<&str>,
    ) -> InstRef {
        let func = self
            .module
            .borrow()
            .as_function_value(callee)
            .expect("expected a function global value");
        self.build_call(func, args, name)
    }

    pub fn build_phi(
        &mut self,
        ty: TypePtr,
        incomings: Vec<(ValueId, BlockRef)>,
        name: Option<&str>,
    ) -> InstRef {
        let func = self.get_current_function();
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
        ptr: ValueId,
        to_ty: TypePtr,
        name: Option<&str>,
    ) -> InstRef {
        self.build_inst(to_ty, InstKind::PtrToInt { ptr }, name)
    }

    pub fn build_trunc(&mut self, value: ValueId, to_ty: TypePtr, name: Option<&str>) -> InstRef {
        self.build_inst(to_ty, InstKind::Trunc { value }, name)
    }

    pub fn build_zext(&mut self, value: ValueId, to_ty: TypePtr, name: Option<&str>) -> InstRef {
        self.build_inst(to_ty, InstKind::Zext { value }, name)
    }

    pub fn build_sext(&mut self, value: ValueId, to_ty: TypePtr, name: Option<&str>) -> InstRef {
        self.build_inst(to_ty, InstKind::Sext { value }, name)
    }

    pub fn build_branch(&mut self, dest: BlockRef) -> InstRef {
        assert_eq!(self.get_current_function(), dest.func);
        self.build_inst(
            Self::void_ty(),
            InstKind::Branch {
                then_block: dest.block,
                cond: None,
            },
            None,
        )
    }

    pub fn build_conditional_branch(
        &mut self,
        cond: ValueId,
        then_block: BlockRef,
        else_block: BlockRef,
    ) -> InstRef {
        let func = self.get_current_function();
        assert_eq!(func, then_block.func);
        assert_eq!(func, else_block.func);
        self.build_inst(
            Self::void_ty(),
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

    pub fn build_return(&mut self, value: Option<ValueId>) -> InstRef {
        self.build_inst(Self::void_ty(), InstKind::Ret { value }, None)
    }

    pub fn build_unreachable(&mut self) -> InstRef {
        self.build_inst(Self::void_ty(), InstKind::Unreachable, None)
    }

    pub fn build_get_size(&self, ty: &TypePtr) -> ValueId {
        let size = self
            .module
            .borrow()
            .size_of(ty)
            .expect("type does not have a concrete layout");
        ValueId::Const(self.module.borrow_mut().add_i32_const(size))
    }

    fn build_mem_intrinsic(
        &mut self,
        name: &str,
        dest: ValueId,
        src: ValueId,
        copy_ty: &TypePtr,
    ) -> InstRef {
        let callee = if let Some(func) = self.module.borrow().get_function(name) {
            func
        } else {
            let func = self
                .module
                .borrow_mut()
                .declare_function_value(name.to_string(), Self::memcpy_ty().into());
            self.module.borrow_mut().append_signature_args(func);
            func
        };
        let size = self.build_get_size(copy_ty);
        let is_volatile = ValueId::Const(self.module.borrow_mut().add_i1_const(false));
        self.build_call(callee, vec![dest, src, size, is_volatile], None)
    }

    pub fn build_memcpy(&mut self, dest: ValueId, src: ValueId, copy_ty: &TypePtr) -> InstRef {
        self.build_mem_intrinsic("llvm.memcpy.p0.p0.i32", dest, src, copy_ty)
    }

    pub fn build_memmove(&mut self, dest: ValueId, src: ValueId, copy_ty: &TypePtr) -> InstRef {
        self.build_mem_intrinsic("llvm.memmove.p0.p0.i32", dest, src, copy_ty)
    }

    pub fn build_bitwise_not(&mut self, operand: ValueId) -> InstRef {
        let ty = self.module.borrow().value_ty(operand).clone();
        let bits = ty
            .as_int()
            .expect("bitwise not expects an integer operand")
            .0;
        let mask = if bits >= 63 {
            i64::MAX
        } else {
            ((1_i64) << bits) - 1
        };
        let mask = ValueId::Const(self.module.borrow_mut().add_int_const(bits, mask));
        self.build_binary(BinaryOpcode::Xor, ty, operand, mask, None)
    }

    pub fn build_neg(&mut self, operand: ValueId) -> InstRef {
        let ty = self.module.borrow().value_ty(operand).clone();
        let bits = ty.as_int().expect("neg expects an integer operand").0;
        let zero = ValueId::Const(self.module.borrow_mut().add_int_const(bits, 0));
        self.build_binary(BinaryOpcode::Sub, ty, zero, operand, None)
    }

    pub fn get_function_value(&self, func: FunctionId) -> Option<ValueId> {
        self.module
            .borrow()
            .get_function_value(func)
            .map(ValueId::Global)
    }

    pub fn get_global_value(&self, name: &str) -> Option<ValueId> {
        self.module.borrow().get_global(name).map(ValueId::Global)
    }

    pub fn get_or_declare_function(
        &mut self,
        name: &str,
        ty: TypePtr,
    ) -> Result<FunctionId, TypePtr> {
        if let Some(func) = self.module.borrow().get_function(name) {
            return Ok(func);
        }
        let Type::Function(FunctionType(ret, args)) = ty.as_ref() else {
            return Err(ty);
        };
        let func_ty: TypePtr = Rc::new(Type::Function(FunctionType(ret.clone(), args.clone())));
        Ok(self
            .module
            .borrow_mut()
            .declare_function_value(name.to_string(), func_ty.into()))
    }
}
