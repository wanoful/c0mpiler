use std::{cell::RefCell, hash::Hash, ops::Deref, rc::Rc};

use enum_as_inner::EnumAsInner;

pub type TypePtr = Rc<Type>;

#[derive(Debug, Clone, PartialEq, Eq, Hash, EnumAsInner)]
pub enum Type {
    Int(IntType),
    Function(FunctionType),
    Ptr(PtrType),
    Struct(StructType),
    Array(ArrayType),
    Void(VoidType),

    Label(LabelType), // basic block 专用
}

impl Type {
    pub fn is_aggregate_type(&self) -> bool {
        matches!(
            self,
            Type::Array(..) | Type::Function(..) | Type::Struct(..)
        )
    }

    pub fn is_zero_length_type(&self) -> bool {
        self.is_void()
            || self
                .as_struct()
                .is_some_and(|x| x.kind.borrow().as_body().is_some_and(|y| y.0.is_empty()))
    }

    pub fn is_fat_ptr(&self) -> bool {
        self.as_struct()
            .is_some_and(|x| x.get_name().is_some_and(|x| x == "fat_ptr"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IntType(pub u8);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionType {
    pub return_type: Rc<Type>,
    pub param_types: Vec<Rc<Type>>,
}

#[derive(Debug, Clone, Eq)]
pub struct StructType {
    pub name: RefCell<Option<String>>,
    pub kind: RefCell<StructTypeEnum>,
}

impl PartialEq for StructType {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

impl Hash for StructType {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        if let Some(name) = self.name.borrow().as_ref() {
            name.hash(state);
        } else {
            let ptr = self.kind.borrow();
            ptr.as_body()
                .unwrap_or_else(|| panic!("StructType has no body!\n {:?}", self))
                .hash(state);
        }
    }
}

impl StructType {
    pub fn set_body(&self, ty: Vec<TypePtr>, packed: bool) {
        (*self.kind.borrow_mut()) = StructTypeEnum::Body { ty, packed };
    }

    pub fn get_body(&self) -> Option<Vec<Rc<Type>>> {
        self.kind.borrow().as_body().map(|x| x.0).cloned()
    }

    pub fn get_name(&self) -> Option<String> {
        self.name.borrow().clone()
    }

    pub fn is_fields_type_same(&self, tys: &[TypePtr]) -> bool {
        let borrowed = self.kind.borrow();
        let Some((body, _)) = borrowed.as_body() else {
            return false;
        };

        body.as_slice() == tys
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EnumAsInner)]
pub enum StructTypeEnum {
    Opaque,
    Body { ty: Vec<Rc<Type>>, packed: bool },
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArrayType(pub Rc<Type>, pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PtrType;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LabelType;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VoidType;

macro_rules! define_extension {
    ($($name:ident),*) => {
        paste::paste!{
            $(
                #[derive(Debug, Clone, PartialEq, Eq, Hash)]
                pub struct [<$name Type Ptr>] (
                    pub(crate) TypePtr
                );

                impl Deref for [<$name Type Ptr>] {
                    type Target = [<$name Type>];

                    fn deref(&self) -> &Self::Target {
                        self.0.[<as_ $name:lower>]().unwrap()
                    }
                }

                impl From<[<$name Type Ptr>]> for TypePtr {
                    fn from(value: [<$name Type Ptr>]) -> Self {
                        value.0
                    }
                }

                impl From<TypePtr> for [<$name Type Ptr>] {
                    fn from(value: TypePtr) -> Self {
                        debug_assert!(value.[<is_ $name:lower>]());
                        Self(value)
                    }
                }
            )*
        }
    };
}

define_extension!(Int, Function, Ptr, Struct, Array, Label, Void);
