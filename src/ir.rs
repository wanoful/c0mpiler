pub mod attribute;
pub mod core;
pub mod core_builder;
pub mod core_inst;
pub mod core_value;
pub mod destructor;
pub mod ir_output;
pub mod ir_type;
pub mod layout;
pub mod opt;
mod cfg;

use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    hash::Hash,
    rc::Rc,
};

use crate::ir::{
    ir_type::{
        ArrayType, ArrayTypePtr, FunctionType, FunctionTypePtr, IntType, IntTypePtr, LabelType,
        PtrType, PtrTypePtr, StructType, StructTypePtr, Type, TypePtr, VoidType,
    },
    layout::TargetDataLayout,
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

#[derive(Debug, Clone)]
pub struct LLVMContext {
    ctx_impl: Rc<RefCell<LLVMContextImpl>>,
}

impl LLVMContext {
    pub fn new(_target: TargetDataLayout) -> Self {
        Self {
            ctx_impl: Rc::new(RefCell::new(LLVMContextImpl::new())),
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

    pub fn named_struct_types(&self) -> HashMap<String, TypePtr> {
        self.ctx_impl.borrow().named_struct_ty.clone()
    }
}

#[derive(Debug, Default)]
struct LLVMContextImpl {
    ty_pool: ContextTypePool,
    named_struct_ty: HashMap<String, TypePtr>,
}

impl LLVMContextImpl {
    fn new() -> Self {
        Self::default()
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

    #[allow(dead_code)]
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
        let ty = Rc::new(Type::Struct(StructType {
            name: RefCell::new(Some(name.to_string())),
            kind: RefCell::new(ir_type::StructTypeEnum::Opaque),
        }));
        self.named_struct_ty.insert(name.to_string(), ty.clone());
        StructTypePtr(ty)
    }

    fn get_named_struct_type(&self, name: &str) -> Option<StructTypePtr> {
        self.named_struct_ty.get(name).cloned().map(StructTypePtr)
    }
}
