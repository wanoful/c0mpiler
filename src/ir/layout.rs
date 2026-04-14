use std::{
    cell::RefCell,
    collections::HashMap,
    error::Error,
    fmt::{self, Display, Formatter},
};

use crate::ir::ir_type::{
    ArrayType, ArrayTypePtr, StructType, StructTypeEnum, StructTypePtr, Type, TypePtr,
};

pub type Size = u32;
pub type Align = u32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Layout {
    pub size: Size,
    pub align: Align,
}

impl Layout {
    pub const fn new(size: Size, align: Align) -> Self {
        Self { size, align }
    }

    pub const fn zero() -> Self {
        Self { size: 0, align: 1 }
    }

    pub fn stride(&self) -> Size {
        align_to(self.size, self.align)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FieldLayout {
    pub offset: Size,
    pub layout: Layout,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StructLayout {
    pub layout: Layout,
    pub fields: Vec<FieldLayout>,
    pub packed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArrayLayout {
    pub layout: Layout,
    pub elem: Layout,
    pub stride: Size,
    pub len: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LayoutShape {
    Scalar,
    Array(ArrayLayout),
    Struct(StructLayout),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeLayout {
    pub layout: Layout,
    pub shape: LayoutShape,
}

impl TypeLayout {
    pub fn as_struct(&self) -> Option<&StructLayout> {
        match &self.shape {
            LayoutShape::Struct(layout) => Some(layout),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&ArrayLayout> {
        match &self.shape {
            LayoutShape::Array(layout) => Some(layout),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TargetDataLayout {
    pub pointer_size: Size,
    pub pointer_align: Align,
}

impl TargetDataLayout {
    pub const fn new(pointer_size: Size, pointer_align: Align) -> Self {
        Self {
            pointer_size,
            pointer_align,
        }
    }

    pub const fn rv32() -> Self {
        Self::new(4, 4)
    }

    pub const fn x86_64() -> Self {
        Self::new(8, 8)
    }

    pub fn llvm_data_layout(&self) -> String {
        let ptr_size_bits = self.pointer_size * 8;
        let ptr_align_bits = self.pointer_align * 8;

        format!("e-m:e-p:{ptr_size_bits}:{ptr_align_bits}-i8:8-i16:16-i32:32-i64:64-n32-S128")
    }
}

impl Default for TargetDataLayout {
    fn default() -> Self {
        Self::rv32()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutError {
    OpaqueStruct { name: Option<String> },
    UnsizedFunction,
    InvalidLabel,
    InvalidFieldIndex { index: usize, field_count: usize },
}

impl Display for LayoutError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            LayoutError::OpaqueStruct { name } => match name {
                Some(name) => write!(f, "opaque struct `{name}` does not have a concrete layout"),
                None => write!(f, "opaque anonymous struct does not have a concrete layout"),
            },
            LayoutError::UnsizedFunction => {
                write!(f, "function types do not have an in-memory layout")
            }
            LayoutError::InvalidLabel => {
                write!(f, "basic block labels do not have an in-memory layout")
            }
            LayoutError::InvalidFieldIndex { index, field_count } => write!(
                f,
                "field index {index} is out of range for struct with {field_count} fields"
            ),
        }
    }
}

impl Error for LayoutError {}

#[derive(Debug, Default)]
pub struct TypeLayoutEngine {
    target: TargetDataLayout,
    cache: RefCell<HashMap<TypePtr, TypeLayout>>,
}

impl TypeLayoutEngine {
    pub fn new(target: TargetDataLayout) -> Self {
        Self {
            target,
            cache: RefCell::default(),
        }
    }

    pub fn target(&self) -> TargetDataLayout {
        self.target
    }

    pub fn clear_cache(&self) {
        self.cache.borrow_mut().clear();
    }

    pub fn has_concrete_layout(&self, ty: &TypePtr) -> bool {
        self.layout_of(ty).is_ok()
    }

    pub fn layout_of(&self, ty: &TypePtr) -> Result<TypeLayout, LayoutError> {
        if let Some(layout) = self.cache.borrow().get(ty).cloned() {
            return Ok(layout);
        }

        let layout = self.compute_layout(ty)?;
        self.cache.borrow_mut().insert(ty.clone(), layout.clone());
        Ok(layout)
    }

    pub fn size_of(&self, ty: &TypePtr) -> Result<Size, LayoutError> {
        Ok(self.layout_of(ty)?.layout.size)
    }

    pub fn align_of(&self, ty: &TypePtr) -> Result<Align, LayoutError> {
        Ok(self.layout_of(ty)?.layout.align)
    }

    pub fn stride_of(&self, ty: &TypePtr) -> Result<Size, LayoutError> {
        Ok(self.layout_of(ty)?.layout.stride())
    }

    pub fn array_layout_of(&self, array_ty: &ArrayTypePtr) -> Result<ArrayLayout, LayoutError> {
        let layout = self.layout_of(&array_ty.clone().into())?;
        Ok(layout.as_array().unwrap().clone())
    }

    pub fn array_offset_of(
        &self,
        array_ty: &ArrayTypePtr,
        index: u32,
    ) -> Result<Size, LayoutError> {
        let layout = self.array_layout_of(array_ty)?;
        Ok(layout.stride * index)
    }

    pub fn struct_layout_of(&self, struct_ty: &StructTypePtr) -> Result<StructLayout, LayoutError> {
        let layout = self.layout_of(&struct_ty.clone().into())?;
        Ok(layout.as_struct().unwrap().clone())
    }

    pub fn field_offset_of(
        &self,
        struct_ty: &StructTypePtr,
        index: usize,
    ) -> Result<Size, LayoutError> {
        let layout = self.struct_layout_of(struct_ty)?;
        layout
            .fields
            .get(index)
            .map(|field| field.offset)
            .ok_or(LayoutError::InvalidFieldIndex {
                index,
                field_count: layout.fields.len(),
            })
    }

    pub fn field_layout_of(
        &self,
        struct_ty: &StructTypePtr,
        index: usize,
    ) -> Result<FieldLayout, LayoutError> {
        let layout = self.struct_layout_of(struct_ty)?;
        layout
            .fields
            .get(index)
            .cloned()
            .ok_or(LayoutError::InvalidFieldIndex {
                index,
                field_count: layout.fields.len(),
            })
    }

    fn compute_layout(&self, ty: &TypePtr) -> Result<TypeLayout, LayoutError> {
        match ty.as_ref() {
            Type::Int(int_ty) => Ok(TypeLayout {
                layout: layout_of_int(int_ty.0),
                shape: LayoutShape::Scalar,
            }),
            Type::Ptr(_) => Ok(TypeLayout {
                layout: Layout::new(self.target.pointer_size, self.target.pointer_align),
                shape: LayoutShape::Scalar,
            }),
            Type::Void(_) => Ok(TypeLayout {
                layout: Layout::zero(),
                shape: LayoutShape::Scalar,
            }),
            Type::Array(ArrayType(elem_ty, len)) => {
                let elem = self.layout_of(elem_ty)?.layout;
                let stride = elem.stride();
                let layout = Layout::new(stride * *len, elem.align);
                Ok(TypeLayout {
                    layout,
                    shape: LayoutShape::Array(ArrayLayout {
                        layout,
                        elem,
                        stride,
                        len: *len,
                    }),
                })
            }
            Type::Struct(struct_ty) => {
                let struct_layout = self.compute_struct_layout(struct_ty)?;
                Ok(TypeLayout {
                    layout: struct_layout.layout,
                    shape: LayoutShape::Struct(struct_layout),
                })
            }
            Type::Function(_) => Err(LayoutError::UnsizedFunction),
            Type::Label(_) => Err(LayoutError::InvalidLabel),
        }
    }

    fn compute_struct_layout(&self, struct_ty: &StructType) -> Result<StructLayout, LayoutError> {
        let borrowed = struct_ty.kind.borrow();
        let (field_tys, packed) = match *borrowed {
            StructTypeEnum::Opaque => {
                return Err(LayoutError::OpaqueStruct {
                    name: struct_ty.get_name(),
                });
            }
            StructTypeEnum::Body { ref ty, packed } => (ty.clone(), packed),
        };
        drop(borrowed);

        let mut fields = Vec::with_capacity(field_tys.len());
        let mut offset = 0;
        let mut struct_align = 1;

        for field_ty in &field_tys {
            let field_layout = self.layout_of(field_ty)?.layout;
            let place_align = if packed { 1 } else { field_layout.align };
            offset = align_to(offset, place_align);
            fields.push(FieldLayout {
                offset,
                layout: field_layout,
            });
            offset += field_layout.size;
            if !packed {
                struct_align = struct_align.max(field_layout.align);
            }
        }

        let align = if packed { 1 } else { struct_align };
        let size = align_to(offset, align);
        Ok(StructLayout {
            layout: Layout::new(size, align),
            fields,
            packed,
        })
    }
}

impl From<TargetDataLayout> for TypeLayoutEngine {
    fn from(target: TargetDataLayout) -> Self {
        Self::new(target)
    }
}

pub fn align_to(value: Size, align: Align) -> Size {
    debug_assert!(align > 0);
    value.next_multiple_of(align)
}

fn layout_of_int(bits: u8) -> Layout {
    let bytes = Size::from(bits.max(1).div_ceil(8));
    let align = match bytes {
        0 | 1 => 1,
        2 => 2,
        3 | 4 => 4,
        _ => 8,
    };
    Layout::new(bytes, align)
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use crate::ir::ir_type::{StructType, StructTypeEnum, Type};

    use super::{Layout, TargetDataLayout, TypeLayoutEngine};

    fn i8() -> Rc<Type> {
        Rc::new(Type::Int(crate::ir::ir_type::IntType(8)))
    }

    fn i32() -> Rc<Type> {
        Rc::new(Type::Int(crate::ir::ir_type::IntType(32)))
    }

    fn ptr() -> Rc<Type> {
        Rc::new(Type::Ptr(crate::ir::ir_type::PtrType))
    }

    fn make_struct(fields: Vec<Rc<Type>>) -> Rc<Type> {
        Rc::new(Type::Struct(StructType {
            name: RefCell::new(None),
            kind: RefCell::new(StructTypeEnum::Body {
                ty: fields,
                packed: false,
            }),
        }))
    }

    #[test]
    fn layout_of_scalars_on_rv32() {
        let engine = TypeLayoutEngine::new(TargetDataLayout::rv32());

        assert_eq!(engine.layout_of(&i8()).unwrap().layout, Layout::new(1, 1));
        assert_eq!(engine.layout_of(&i32()).unwrap().layout, Layout::new(4, 4));
        assert_eq!(engine.layout_of(&ptr()).unwrap().layout, Layout::new(4, 4));
    }

    #[test]
    fn layout_of_array_uses_element_stride() {
        let engine = TypeLayoutEngine::new(TargetDataLayout::rv32());
        let array = Rc::new(Type::Array(crate::ir::ir_type::ArrayType(i32(), 10)));

        let layout = engine.layout_of(&array).unwrap();
        let array_layout = layout.as_array().unwrap();

        assert_eq!(array_layout.elem, Layout::new(4, 4));
        assert_eq!(array_layout.stride, 4);
        assert_eq!(array_layout.layout, Layout::new(40, 4));
    }

    #[test]
    fn layout_of_struct_tracks_field_offsets() {
        let engine = TypeLayoutEngine::new(TargetDataLayout::rv32());
        let ty = make_struct(vec![i8(), i32(), i8()]);
        let struct_layout = engine.layout_of(&ty).unwrap().as_struct().unwrap().clone();

        assert_eq!(struct_layout.fields[0].offset, 0);
        assert_eq!(struct_layout.fields[1].offset, 4);
        assert_eq!(struct_layout.fields[2].offset, 8);
        assert_eq!(struct_layout.layout, Layout::new(12, 4));
    }

    #[test]
    fn fat_ptr_like_struct_is_two_words() {
        let engine = TypeLayoutEngine::new(TargetDataLayout::rv32());
        let fat_ptr = make_struct(vec![ptr(), i32()]);
        let struct_layout = engine
            .layout_of(&fat_ptr)
            .unwrap()
            .as_struct()
            .unwrap()
            .clone();

        assert_eq!(struct_layout.fields[0].offset, 0);
        assert_eq!(struct_layout.fields[1].offset, 4);
        assert_eq!(struct_layout.layout, Layout::new(8, 4));
    }

    #[test]
    fn packed_struct_ignores_field_alignment() {
        let engine = TypeLayoutEngine::new(TargetDataLayout::rv32());
        let ty = Rc::new(Type::Struct(StructType {
            name: RefCell::new(None),
            kind: RefCell::new(StructTypeEnum::Body {
                ty: vec![i8(), i32()],
                packed: true,
            }),
        }));
        let struct_layout = engine.layout_of(&ty).unwrap().as_struct().unwrap().clone();

        assert_eq!(struct_layout.fields[0].offset, 0);
        assert_eq!(struct_layout.fields[1].offset, 1);
        assert_eq!(struct_layout.layout, Layout::new(5, 1));
    }
}
