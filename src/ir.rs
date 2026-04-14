pub mod attribute;
pub mod destructor;
pub mod globalxxx;
pub mod ir_output;
pub mod ir_type;
pub mod ir_value;
pub mod layout;

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    hash::Hash,
    rc::Rc,
    vec,
};

use crate::ir::{
    attribute::FunctionAttribute,
    globalxxx::{FunctionPtr, GlobalObject, GlobalVariable, GlobalVariablePtr},
    ir_type::{
        ArrayType, ArrayTypePtr, FunctionType, FunctionTypePtr, IntType, IntTypePtr, LabelType,
        PtrType, PtrTypePtr, StructType, StructTypePtr, Type, TypePtr, VoidType,
    },
    ir_value::{
        Argument, ArgumentPtr, BasicBlock, BasicBlockPtr, BinaryOpcode, Constant, ConstantArray,
        ConstantArrayPtr, ConstantInt, ConstantIntPtr, ConstantNull, ConstantNullPtr, ConstantPtr,
        ConstantString, ConstantStringPtr, ConstantStruct, ConstantStructPtr, GlobalObjectPtr,
        ICmpCode, Instruction, InstructionPtr, Value, ValueBase, ValuePtr,
    },
    layout::{TargetDataLayout, TypeLayoutEngine},
};

#[derive(Debug)]
struct ContextPool<T>(HashSet<Rc<T>>);

impl<T> Default for ContextPool<T> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<T> ContextPool<T>
where
    T: Eq + Hash + Clone,
{
    fn get_ty(&mut self, ty: &T) -> Rc<T> {
        if let Some(ret) = self.0.get(ty) {
            ret.clone()
        } else {
            let ret = Rc::new(ty.clone());
            self.0.insert(Rc::new(ty.clone()));
            ret
        }
    }
}

type ContextTypePool = ContextPool<Type>;

impl ContextTypePool {
    fn int_type(&mut self, bit_width: u8) -> TypePtr {
        self.get_ty(&Type::Int(IntType(bit_width)))
    }

    fn i1_type(&mut self) -> TypePtr {
        self.int_type(1)
    }

    fn i8_type(&mut self) -> TypePtr {
        self.int_type(8)
    }

    fn i32_type(&mut self) -> TypePtr {
        self.int_type(32)
    }

    fn void_type(&mut self) -> TypePtr {
        self.get_ty(&Type::Void(VoidType))
    }

    fn ptr_type(&mut self) -> TypePtr {
        self.get_ty(&Type::Ptr(PtrType))
    }

    fn label_type(&mut self) -> TypePtr {
        self.get_ty(&Type::Label(LabelType))
    }
}

// 存放符号表，常量等
#[derive(Debug, Default)]
pub struct LLVMContext {
    ctx_impl: Rc<RefCell<LLVMContextImpl>>,
}

impl LLVMContext {
    pub fn new(target: TargetDataLayout) -> Self {
        Self {
            ctx_impl: Rc::new(RefCell::new(LLVMContextImpl::new(target))),
        }
    }

    pub fn create_builder(&self) -> LLVMBuilder {
        LLVMBuilder {
            ctx_impl: self.ctx_impl.clone(),
            target_block: None,
        }
    }

    pub fn create_module(&mut self, name: &str) -> LLVMModule {
        LLVMModule {
            _name: name.to_string(),
            ctx_impl: self.ctx_impl.clone(),
            global_variables: HashMap::default(),
            functions: HashMap::default(),
        }
    }

    pub fn i1_type(&self) -> IntTypePtr {
        self.ctx_impl.borrow_mut().i1_type()
    }

    pub fn i8_type(&self) -> IntTypePtr {
        self.ctx_impl.borrow_mut().i8_type()
    }

    pub fn i32_type(&self) -> IntTypePtr {
        self.ctx_impl.borrow_mut().i32_type()
    }

    pub fn void_type(&self) -> IntTypePtr {
        self.ctx_impl.borrow_mut().void_type()
    }

    pub fn ptr_type(&self) -> PtrTypePtr {
        self.ctx_impl.borrow_mut().ptr_type()
    }

    pub fn array_type(&self, inner_type: TypePtr, length: u32) -> ArrayTypePtr {
        self.ctx_impl.borrow_mut().array_type(inner_type, length)
    }

    pub fn struct_type(&self, inner_types: Vec<TypePtr>, packed: bool) -> StructTypePtr {
        self.ctx_impl.borrow_mut().struct_type(inner_types, packed)
    }

    pub fn function_type(&self, ret_type: TypePtr, arg_tys: Vec<TypePtr>) -> FunctionTypePtr {
        self.ctx_impl.borrow_mut().function_type(ret_type, arg_tys)
    }

    pub fn create_opaque_struct_type(&self, name: &str) -> StructTypePtr {
        self.ctx_impl.borrow_mut().create_opaque_struct_type(name)
    }

    pub fn get_named_struct_type(&self, name: &str) -> Option<StructTypePtr> {
        self.ctx_impl.borrow().get_named_struct_type(name)
    }

    pub fn get_i1(&self, value: bool) -> ConstantIntPtr {
        self.ctx_impl.borrow_mut().get_i1(value)
    }

    pub fn get_true(&self) -> ConstantIntPtr {
        self.ctx_impl.borrow_mut().get_true()
    }

    pub fn get_false(&self) -> ConstantIntPtr {
        self.ctx_impl.borrow_mut().get_false()
    }

    pub fn get_i8(&self, value: u8) -> ConstantIntPtr {
        self.ctx_impl.borrow_mut().get_i8(value)
    }

    pub fn get_i32(&self, value: u32) -> ConstantIntPtr {
        self.ctx_impl.borrow_mut().get_i32(value)
    }

    pub fn get_null(&self) -> ConstantNullPtr {
        self.ctx_impl.borrow_mut().get_null()
    }

    pub fn get_array(&self, inner_ty: TypePtr, values: Vec<ConstantPtr>) -> ConstantArrayPtr {
        self.ctx_impl.borrow_mut().get_array(inner_ty, values)
    }

    pub fn get_struct(
        &self,
        struct_ty: StructTypePtr,
        values: Vec<ConstantPtr>,
    ) -> ConstantStructPtr {
        self.ctx_impl.borrow_mut().get_struct(struct_ty, values)
    }

    pub fn get_string(&self, string: &str) -> ConstantStringPtr {
        self.ctx_impl.borrow_mut().get_string(string)
    }

    pub fn append_basic_block(&self, func: &FunctionPtr, name: &str) -> BasicBlockPtr {
        let mut blocks = func.as_function().blocks.borrow_mut();
        let label_ty = self.ctx_impl.borrow_mut().label_type();

        let block = BasicBlockPtr(
            Value {
                base: ValueBase::new(label_ty, Some(name)),
                kind: ir_value::ValueKind::BasicBlock(BasicBlock {
                    instructions: RefCell::default(),
                }),
            }
            .into(),
        );

        blocks.push(block.clone());

        block
    }
}

#[derive(Debug, Default)]
pub struct LLVMContextImpl {
    ty_pool: ContextTypePool,
    named_strcut_ty: HashMap<String, TypePtr>,

    type_layout_engine: TypeLayoutEngine,

    // 常量才会被缓存，指向唯一地址
    // 所以常量从 context 获取，其余从 builder 获取
    integer_pool: HashMap<(u8, u32), ConstantIntPtr>, // (位数, 大小)
    array_pool: HashMap<(TypePtr, Vec<ConstantPtr>), ConstantArrayPtr>,
    struct_pool: HashMap<Vec<ConstantPtr>, ConstantStructPtr>,
    string_pool: HashMap<String, ConstantStringPtr>,
    null: Option<ConstantNullPtr>,
}

impl LLVMContextImpl {
    pub fn new(target: TargetDataLayout) -> Self {
        Self {
            type_layout_engine: TypeLayoutEngine::new(target),
            ..Default::default()
        }
    }

    fn int_type(&mut self, bit_width: u8) -> IntTypePtr {
        IntTypePtr(self.ty_pool.int_type(bit_width))
    }

    fn i1_type(&mut self) -> IntTypePtr {
        IntTypePtr(self.ty_pool.i1_type())
    }

    fn i8_type(&mut self) -> IntTypePtr {
        IntTypePtr(self.ty_pool.i8_type())
    }

    fn i32_type(&mut self) -> IntTypePtr {
        IntTypePtr(self.ty_pool.i32_type())
    }

    fn void_type(&mut self) -> IntTypePtr {
        IntTypePtr(self.ty_pool.void_type())
    }

    fn ptr_type(&mut self) -> PtrTypePtr {
        PtrTypePtr(self.ty_pool.ptr_type())
    }

    fn label_type(&mut self) -> TypePtr {
        self.ty_pool.label_type()
    }

    fn array_type(&mut self, inner_type: TypePtr, length: u32) -> ArrayTypePtr {
        ArrayTypePtr(
            self.ty_pool
                .get_ty(&Type::Array(ArrayType(inner_type, length))),
        )
    }

    fn struct_type(&mut self, inner_types: Vec<TypePtr>, packed: bool) -> StructTypePtr {
        StructTypePtr(self.ty_pool.get_ty(&Type::Struct(StructType {
            name: RefCell::new(None),
            kind: RefCell::new(ir_type::StructTypeEnum::Body {
                ty: inner_types,
                packed,
            }),
        })))
    }

    fn function_type(&mut self, ret_type: TypePtr, arg_tys: Vec<TypePtr>) -> FunctionTypePtr {
        FunctionTypePtr(
            self.ty_pool
                .get_ty(&Type::Function(FunctionType(ret_type, arg_tys))),
        )
    }

    fn create_opaque_struct_type(&mut self, name: &str) -> StructTypePtr {
        let ret = Rc::new(Type::Struct(StructType {
            name: RefCell::new(Some(name.to_string())),
            kind: RefCell::new(ir_type::StructTypeEnum::Opaque),
        }));
        self.named_strcut_ty.insert(name.to_string(), ret.clone());
        StructTypePtr(ret)
    }

    fn get_named_struct_type(&self, name: &str) -> Option<StructTypePtr> {
        self.named_strcut_ty.get(name).cloned().map(StructTypePtr)
    }

    fn get_int(&mut self, value: u32, bit_width: u8) -> ConstantIntPtr {
        if let Some(ret) = self.integer_pool.get(&(bit_width, value)) {
            ret.clone()
        } else {
            let ret = ConstantIntPtr(ConstantPtr(ValuePtr::new(Value {
                base: ValueBase::new(self.int_type(bit_width).into(), None),
                kind: ir_value::ValueKind::Constant(ir_value::Constant::ConstantInt(ConstantInt(
                    value,
                ))),
            })));
            self.integer_pool.insert((bit_width, value), ret.clone());
            ret
        }
    }

    fn get_i1(&mut self, value: bool) -> ConstantIntPtr {
        self.get_int(value as u32, 1)
    }

    fn get_true(&mut self) -> ConstantIntPtr {
        self.get_int(1, 1)
    }

    fn get_false(&mut self) -> ConstantIntPtr {
        self.get_int(0, 1)
    }

    fn get_i8(&mut self, value: u8) -> ConstantIntPtr {
        self.get_int(value as u32, 8)
    }

    fn get_i32(&mut self, value: u32) -> ConstantIntPtr {
        self.get_int(value, 32)
    }

    fn get_null(&mut self) -> ConstantNullPtr {
        if let Some(null) = &self.null {
            null.clone()
        } else {
            let t = ConstantNullPtr(ConstantPtr(ValuePtr::new(Value {
                base: ValueBase::new(self.ptr_type().into(), None),
                kind: ir_value::ValueKind::Constant(Constant::ConstantNull(ConstantNull)),
            })));

            self.null = Some(t.clone());
            t
        }
    }

    fn get_array(&mut self, inner_ty: TypePtr, values: Vec<ConstantPtr>) -> ConstantArrayPtr {
        debug_assert!(values.iter().all(|x| *x.get_type() == inner_ty));

        let key = (inner_ty, values);

        if let Some(ret) = self.array_pool.get(&key) {
            ret.clone()
        } else {
            let array_ty = self.array_type(key.0.clone(), key.1.len() as u32);

            let ret = ConstantArrayPtr(ConstantPtr(
                Value {
                    base: ValueBase::new(array_ty.into(), None),
                    kind: ir_value::ValueKind::Constant(ir_value::Constant::ConstantArray(
                        ConstantArray(key.1.clone()),
                    )),
                }
                .into(),
            ));

            self.array_pool.insert(key, ret.clone());

            ret
        }
    }

    fn get_struct(
        &mut self,
        struct_ty: StructTypePtr,
        values: Vec<ConstantPtr>,
    ) -> ConstantStructPtr {
        debug_assert!(
            struct_ty.is_fields_type_same(
                &values
                    .iter()
                    .map(|x| x.get_type().clone())
                    .collect::<Vec<_>>()
            )
        );

        if let Some(ret) = self.struct_pool.get(&values) {
            ret.clone()
        } else {
            let ret = ConstantStructPtr(ConstantPtr(
                Value {
                    base: ValueBase::new(struct_ty.into(), None),
                    kind: ir_value::ValueKind::Constant(ir_value::Constant::ConstantStruct(
                        ConstantStruct(values.clone()),
                    )),
                }
                .into(),
            ));

            self.struct_pool.insert(values, ret.clone());

            ret
        }
    }

    fn get_string(&mut self, string: &str) -> ConstantStringPtr {
        if let Some(ret) = self.string_pool.get(string) {
            ret.clone()
        } else {
            let i8_type = self.i8_type();
            let array_ty = self.array_type(i8_type.into(), string.len() as u32);

            let ret = ConstantStringPtr(ConstantPtr(
                Value {
                    base: ValueBase::new(array_ty.into(), None),
                    kind: ir_value::ValueKind::Constant(ir_value::Constant::ConstantString(
                        ConstantString(string.to_string()),
                    )),
                }
                .into(),
            ));

            self.string_pool.insert(string.to_string(), ret.clone());

            ret
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum BuilderInsertPlace {
    FRONT,
    END,
}

#[derive(Debug, Clone)]
pub struct BuilderLocation {
    fn_ptr: FunctionPtr,
    bb_ptr: BasicBlockPtr,
    insert_place: BuilderInsertPlace,
}

#[derive(Debug)]
pub struct LLVMBuilder {
    ctx_impl: Rc<RefCell<LLVMContextImpl>>,
    target_block: Option<BuilderLocation>,
}

impl LLVMBuilder {
    pub fn get_location(&self) -> Option<BuilderLocation> {
        self.target_block.clone()
    }

    pub fn set_location(&mut self, location: Option<BuilderLocation>) {
        self.target_block = location;
    }

    pub fn get_current_function(&self) -> &FunctionPtr {
        &self.target_block.as_ref().unwrap().fn_ptr
    }

    pub fn get_current_basic_block(&self) -> &BasicBlockPtr {
        &self.target_block.as_ref().unwrap().bb_ptr
    }

    pub fn locate_end(&mut self, target_function: FunctionPtr, target_block: BasicBlockPtr) {
        self.target_block = Some(BuilderLocation {
            fn_ptr: target_function,
            bb_ptr: target_block,
            insert_place: BuilderInsertPlace::END,
        });
    }

    pub fn locate_front(&mut self, target_function: FunctionPtr, target_block: BasicBlockPtr) {
        self.target_block = Some(BuilderLocation {
            fn_ptr: target_function,
            bb_ptr: target_block,
            insert_place: BuilderInsertPlace::FRONT,
        });
    }

    fn insert(&self, ins: InstructionPtr) -> InstructionPtr {
        let builder_location = self.target_block.as_ref().unwrap();
        let bb = builder_location.bb_ptr.as_basic_block();
        let mut borrow_mut = bb.instructions.borrow_mut();

        match builder_location.insert_place {
            BuilderInsertPlace::FRONT => {
                if !borrow_mut.is_empty() && ins.as_instruction().is_terminate() {
                    panic!("Can't insert terminator at the front of basic block!");
                }
                borrow_mut.push_front(ins.clone())
            }
            BuilderInsertPlace::END => {
                if let Some(last) = borrow_mut.back()
                    && last.as_instruction().is_terminate()
                {
                    panic!("The basic block has been terminated!");
                }
                borrow_mut.push_back(ins.clone())
            }
        }

        for x in &ins.as_instruction().operands {
            x.add_user(&ins);
        }
        ins
    }

    pub fn build_alloca(&self, ty: TypePtr, name: Option<&str>) -> InstructionPtr {
        let ptr_type = self.ctx_impl.borrow_mut().ptr_type();

        self.insert(InstructionPtr(
            Value {
                base: ValueBase::new(ptr_type.into(), name),
                kind: ir_value::ValueKind::Instruction(Instruction {
                    kind: ir_value::InstructionKind::Alloca { inner_ty: ty },
                    operands: vec![],
                }),
            }
            .into(),
        ))
    }

    pub fn build_load(&self, ty: TypePtr, ptr: ValuePtr, name: Option<&str>) -> InstructionPtr {
        debug_assert!(ptr.is_ptr_type());

        self.insert(InstructionPtr(
            Value {
                base: ValueBase::new(ty, name),
                kind: ir_value::ValueKind::Instruction(Instruction {
                    kind: ir_value::InstructionKind::Load,
                    operands: vec![ptr],
                }),
            }
            .into(),
        ))
    }

    pub fn build_store(&self, value: ValuePtr, ptr: ValuePtr) -> InstructionPtr {
        debug_assert!(ptr.is_ptr_type());

        let void_type = self.ctx_impl.borrow_mut().void_type();

        self.insert(InstructionPtr(
            Value {
                base: ValueBase::new(void_type.into(), None),
                kind: ir_value::ValueKind::Instruction(Instruction {
                    kind: ir_value::InstructionKind::Store,
                    operands: vec![value, ptr],
                }),
            }
            .into(),
        ))
    }

    pub fn build_conditional_branch(
        &self,
        cond: ValuePtr,
        iftrue: BasicBlockPtr,
        ifelse: BasicBlockPtr,
    ) -> InstructionPtr {
        debug_assert!({
            let mut ctx = self.ctx_impl.borrow_mut();
            let i1_type = ctx.i1_type();
            *cond.get_type() == i1_type.into()
        });

        let void_type = self.ctx_impl.borrow_mut().void_type();

        self.insert(InstructionPtr(
            Value {
                base: ValueBase::new(void_type.into(), None),
                kind: ir_value::ValueKind::Instruction(Instruction {
                    kind: ir_value::InstructionKind::Branch { has_cond: true },
                    operands: vec![cond, iftrue.into(), ifelse.into()],
                }),
            }
            .into(),
        ))
    }

    pub fn build_branch(&self, dest: BasicBlockPtr) -> InstructionPtr {
        let void_type = self.ctx_impl.borrow_mut().void_type();

        self.insert(InstructionPtr(
            Value {
                base: ValueBase::new(void_type.into(), None),
                kind: ir_value::ValueKind::Instruction(Instruction {
                    kind: ir_value::InstructionKind::Branch { has_cond: false },
                    operands: vec![dest.into()],
                }),
            }
            .into(),
        ))
    }

    pub fn build_return(&self, value: Option<ValuePtr>) -> InstructionPtr {
        let void_type = self.ctx_impl.borrow_mut().void_type();

        self.insert(InstructionPtr(
            Value {
                base: ValueBase::new(void_type.into(), None),
                kind: ir_value::ValueKind::Instruction(Instruction {
                    kind: ir_value::InstructionKind::Ret {
                        is_void: value.is_none(),
                    },
                    operands: value.into_iter().collect(),
                }),
            }
            .into(),
        ))
    }

    pub fn build_unreachable(&self) -> InstructionPtr {
        let void_type = self.ctx_impl.borrow_mut().void_type();

        self.insert(InstructionPtr(
            Value {
                base: ValueBase::new(void_type.into(), None),
                kind: ir_value::ValueKind::Instruction(Instruction {
                    kind: ir_value::InstructionKind::Unreachable,
                    operands: vec![],
                }),
            }
            .into(),
        ))
    }

    pub fn build_getelementptr(
        &self,
        base_ty: TypePtr,
        ptr: ValuePtr,
        gets: Vec<ValuePtr>,
        name: Option<&str>,
    ) -> InstructionPtr {
        debug_assert!(ptr.is_ptr_type());

        let ptr_type = self.ctx_impl.borrow_mut().ptr_type();

        self.insert(InstructionPtr(
            Value {
                base: ValueBase::new(ptr_type.into(), name),
                kind: ir_value::ValueKind::Instruction(Instruction {
                    kind: ir_value::InstructionKind::GetElementPtr { base_ty },
                    operands: [vec![ptr], gets].concat(),
                }),
            }
            .into(),
        ))
    }

    pub fn build_icmp(
        &self,
        cond: ICmpCode,
        value1: ValuePtr,
        value2: ValuePtr,
        name: Option<&str>,
    ) -> InstructionPtr {
        debug_assert!(
            value1.get_type() == value2.get_type(),
            "{:?}\n{:?}",
            value1,
            value2
        );

        let bool_type = self.ctx_impl.borrow_mut().i1_type();

        self.insert(InstructionPtr(
            Value {
                base: ValueBase::new(bool_type.into(), name),
                kind: ir_value::ValueKind::Instruction(Instruction {
                    kind: ir_value::InstructionKind::Icmp(cond),
                    operands: vec![value1, value2],
                }),
            }
            .into(),
        ))
    }

    pub fn build_call(
        &self,
        func: FunctionPtr,
        args: Vec<ValuePtr>,
        name: Option<&str>,
    ) -> InstructionPtr {
        let func_type = func
            .as_global_object()
            .get_inner_ty()
            .as_function()
            .unwrap();

        debug_assert!(
            {
                let target_tys = &func_type.1;
                target_tys.len() == args.len()
                    && target_tys
                        .iter()
                        .zip(args.iter())
                        .all(|(x, y)| x == y.get_type())
            },
            "Call argument mismatch!\nfunc:{:#?}\nargs:{:#?}",
            func,
            args
        );

        let ret_ty = func_type.0.clone();

        self.insert(InstructionPtr(
            Value {
                base: ValueBase::new(ret_ty, name),
                kind: ir_value::ValueKind::Instruction(Instruction {
                    kind: ir_value::InstructionKind::Call,
                    operands: [vec![func.into()], args].concat(),
                }),
            }
            .into(),
        ))
    }

    pub fn build_phi(
        &self,
        ty: TypePtr,
        froms: Vec<(ValuePtr, BasicBlockPtr)>,
        name: Option<&str>,
    ) -> InstructionPtr {
        debug_assert!(froms.iter().all(|(t, _)| *t.get_type() == ty));

        self.insert(InstructionPtr(
            Value {
                base: ValueBase::new(ty, name),
                kind: ir_value::ValueKind::Instruction(Instruction {
                    kind: ir_value::InstructionKind::Phi,
                    operands: froms
                        .into_iter()
                        .flat_map(|(x, y)| [x, y.into()].into_iter())
                        .collect(),
                }),
            }
            .into(),
        ))
    }

    pub fn build_select(
        &self,
        cond: ValuePtr,
        value1: ValuePtr,
        value2: ValuePtr,
        name: Option<&str>,
    ) -> InstructionPtr {
        debug_assert!(cond.is_array_type() && value1.get_type() == value2.get_type());

        let ty = value1.get_type().clone();

        self.insert(InstructionPtr(
            Value {
                base: ValueBase::new(ty, name),
                kind: ir_value::ValueKind::Instruction(Instruction {
                    kind: ir_value::InstructionKind::Select,
                    operands: vec![cond, value1, value2],
                }),
            }
            .into(),
        ))
    }

    pub fn build_binary(
        &self,
        operator: BinaryOpcode,
        ty: TypePtr,
        value1: ValuePtr,
        value2: ValuePtr,
        name: Option<&str>,
    ) -> InstructionPtr {
        debug_assert!(value1.get_type() == value2.get_type());

        self.insert(InstructionPtr(
            Value {
                base: ValueBase::new(ty, name),
                kind: ir_value::ValueKind::Instruction(Instruction {
                    kind: ir_value::InstructionKind::Binary(operator),
                    operands: vec![value1, value2],
                }),
            }
            .into(),
        ))
    }

    pub fn build_ptr_to_int(
        &self,
        value: ValuePtr,
        to_ty: IntTypePtr,
        name: Option<&str>,
    ) -> InstructionPtr {
        self.insert(InstructionPtr(
            Value {
                base: ValueBase::new(to_ty.into(), name),
                kind: ir_value::ValueKind::Instruction(Instruction {
                    kind: ir_value::InstructionKind::PtrToInt,
                    operands: vec![value],
                }),
            }
            .into(),
        ))
    }

    pub fn build_get_size(&self, ty: TypePtr) -> ConstantIntPtr {
        let mut ctx = self.ctx_impl.borrow_mut();
        let type_layout = ctx.type_layout_engine.layout_of(&ty).unwrap();
        let size = type_layout.layout.size;
        ctx.get_i32(size)
    }

    pub fn build_memcpy(
        &self,
        module: &mut LLVMModule,
        dest: ValuePtr,
        src: ValuePtr,
        ty: TypePtr,
    ) -> InstructionPtr {
        const NAME: &str = "llvm.memcpy.p0.p0.i32";
        self.build_memxxx(module, dest, src, ty, NAME)
    }

    pub fn build_memmove(
        &self,
        module: &mut LLVMModule,
        dest: ValuePtr,
        src: ValuePtr,
        ty: TypePtr,
    ) -> InstructionPtr {
        const NAME: &str = "llvm.memmove.p0.p0.i32";
        self.build_memxxx(module, dest, src, ty, NAME)
    }

    fn build_memxxx(
        &self,
        module: &mut LLVMModule,
        dest: Rc<Value>,
        src: Rc<Value>,
        ty: Rc<Type>,
        name: &str,
    ) -> InstructionPtr {
        let f = if let Some(f) = module.get_function(name) {
            f
        } else {
            let mut ctx = self.ctx_impl.borrow_mut();
            let void_type = ctx.void_type();
            let ptr_type = ctx.ptr_type();
            let i32_type = ctx.i32_type();
            let i1_type = ctx.i1_type();
            let t = ctx.function_type(
                void_type.into(),
                vec![
                    ptr_type.clone().into(),
                    ptr_type.into(),
                    i32_type.into(),
                    i1_type.into(),
                ],
            );
            drop(ctx);
            module.add_function(t, name, None)
        };

        let size = self.build_get_size(ty);

        self.build_call(
            f,
            vec![
                dest,
                src,
                size.into(),
                self.ctx_impl.borrow_mut().get_i1(false).into(),
            ],
            None,
        )
    }

    pub fn build_bitwise_not(&self, operant: ValuePtr) -> InstructionPtr {
        let ty = operant.get_type();
        let bits = ty.as_int().unwrap().0;
        let mut ctx = self.ctx_impl.borrow_mut();
        let mask = ctx.get_int(1u32.unbounded_shl(bits as u32).overflowing_sub(1).0, bits);
        self.build_binary(BinaryOpcode::Xor, ty.clone(), operant, mask.into(), None)
    }

    pub fn build_neg(&self, operant: ValuePtr) -> InstructionPtr {
        let ty = operant.get_type();
        let bits = ty.as_int().unwrap().0;
        let mut ctx = self.ctx_impl.borrow_mut();
        let zero = ctx.get_int(0, bits);
        self.build_binary(BinaryOpcode::Sub, ty.clone(), zero.into(), operant, None)
    }

    pub fn build_zext(&self, from: ValuePtr, to: TypePtr, name: Option<&str>) -> InstructionPtr {
        self.insert(InstructionPtr(
            Value {
                base: ValueBase::new(to, name),
                kind: ir_value::ValueKind::Instruction(Instruction {
                    kind: ir_value::InstructionKind::Zext,
                    operands: vec![from],
                }),
            }
            .into(),
        ))
    }

    pub fn build_sext(&self, from: ValuePtr, to: TypePtr, name: Option<&str>) -> InstructionPtr {
        self.insert(InstructionPtr(
            Value {
                base: ValueBase::new(to, name),
                kind: ir_value::ValueKind::Instruction(Instruction {
                    kind: ir_value::InstructionKind::Sext,
                    operands: vec![from],
                }),
            }
            .into(),
        ))
    }

    pub fn build_trunc(&self, from: ValuePtr, to: TypePtr, name: Option<&str>) -> InstructionPtr {
        self.insert(InstructionPtr(
            Value {
                base: ValueBase::new(to, name),
                kind: ir_value::ValueKind::Instruction(Instruction {
                    kind: ir_value::InstructionKind::Trunc,
                    operands: vec![from],
                }),
            }
            .into(),
        ))
    }
}

#[derive(Debug)]
pub struct LLVMModule {
    _name: String,
    ctx_impl: Rc<RefCell<LLVMContextImpl>>,

    // 复杂常量（数组、结构体）等还是需要放在 global_variables 中，因为它们使用 GEP 操作，从而需要一个地址
    global_variables: HashMap<String, GlobalVariablePtr>,
    functions: HashMap<String, (FunctionPtr, usize)>,
}

impl LLVMModule {
    pub fn add_function(
        &mut self,
        func_ty: FunctionTypePtr,
        name: &str,
        args_name: Option<Vec<String>>,
    ) -> FunctionPtr {
        let mut params: Vec<ArgumentPtr> = func_ty
            .1
            .iter()
            .map(|x| {
                ArgumentPtr(
                    Value {
                        base: ValueBase::new(x.clone(), None),
                        kind: ir_value::ValueKind::Argument(Argument),
                    }
                    .into(),
                )
            })
            .collect();

        if let Some(args_name) = args_name {
            debug_assert!(params.len() == args_name.len());

            params
                .iter_mut()
                .zip(args_name)
                .for_each(|(param, name)| param.set_name(name));
        }

        let param_num = params.len();
        let func = FunctionPtr(GlobalObjectPtr(
            Value {
                base: ValueBase::new(self.ctx_impl.borrow_mut().ptr_type().into(), Some(name)),
                kind: ir_value::ValueKind::GlobalObject(GlobalObject {
                    inner_ty: func_ty.into(),
                    kind: globalxxx::GlobalObjectKind::Function(globalxxx::Function {
                        params,
                        blocks: RefCell::default(),
                        attr: FunctionAttribute::new(param_num).into(),
                    }),
                }),
            }
            .into(),
        ));

        let len = self.functions.len();
        self.functions.insert(name.to_string(), (func.clone(), len));

        func
    }

    pub fn get_function(&self, name: &str) -> Option<FunctionPtr> {
        self.functions.get(name).cloned().map(|(f, _)| f)
    }

    pub fn add_global_variable(
        &mut self,
        is_constant: bool,
        initializer: ConstantPtr,
        name: &str,
    ) -> GlobalVariablePtr {
        let var = GlobalVariablePtr(GlobalObjectPtr(
            Value {
                base: ValueBase::new(self.ctx_impl.borrow_mut().ptr_type().into(), Some(name)),
                kind: ir_value::ValueKind::GlobalObject(GlobalObject {
                    inner_ty: initializer.get_type().clone(),
                    kind: globalxxx::GlobalObjectKind::GlobalVariable(GlobalVariable {
                        is_constant,
                        initializer,
                    }),
                }),
            }
            .into(),
        ));

        self.global_variables.insert(name.to_string(), var.clone());

        var
    }

    pub fn get_global_variable(&mut self, name: &str) -> Option<GlobalVariablePtr> {
        self.global_variables.get(name).cloned()
    }
}

#[test]
fn foo() {
    let mut context = LLVMContext::default();
    let mut builder = context.create_builder();
    let mut module = context.create_module("module");

    let struct_type = context.create_opaque_struct_type("Struct");
    struct_type.set_body(
        vec![context.i32_type().into(), context.i32_type().into()],
        false,
    );

    let gv = module.add_global_variable(
        true,
        context
            .get_struct(
                struct_type.clone(),
                vec![context.get_i32(114).into(), context.get_i32(514).into()],
            )
            .into(),
        "Struct",
    );

    let i32_type = context.i32_type();

    let function_type = context.function_type(i32_type.clone().into(), vec![]);
    let foo_type =
        context.function_type(context.void_type().into(), vec![context.i32_type().into()]);

    let func = module.add_function(function_type, "main", None);
    let foo = module.add_function(foo_type, "printlnInt", None);

    let bb = context.append_basic_block(&func, "entry");

    builder.locate_end(func.clone(), bb);

    let addee_ptr = builder.build_getelementptr(
        struct_type.clone().into(),
        gv.into(),
        vec![context.get_i32(0).into(), context.get_i32(0).into()],
        None,
    );
    let addee = builder.build_load(context.i32_type().into(), addee_ptr.into(), None);

    let sum = builder.build_binary(
        BinaryOpcode::Add,
        i32_type.clone().into(),
        context.get_i32(3).into(),
        addee.into(),
        None,
    );

    builder.build_call(foo, vec![sum.into()], None);

    builder.build_return(Some(context.get_i32(0).into()));

    let result = module.print();
    println!("{result}");
}
