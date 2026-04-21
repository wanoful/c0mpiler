use enum_as_inner::EnumAsInner;

use crate::{
    ir::{
        core::ValueId,
        ir_type::TypePtr,
    },
    irgen::IRGenerator,
    semantics::{
        impls::DerefLevel,
        resolved_ty::{ResolvedTy, ResolvedTyKind},
        value::{PlaceValueIndex, ValueIndex, ValueIndexKind},
    },
};

#[derive(Debug, Clone)]
pub(crate) struct ValuePtrContainer {
    pub(crate) value_ptr: ValueId,
    pub(crate) kind: ContainerKind,
}

impl ValuePtrContainer {
    pub(crate) fn flatten(self) -> Vec<ValueId> {
        match self.kind {
            ContainerKind::Raw { fat: Some(fat) } => vec![self.value_ptr, fat],
            _ => vec![self.value_ptr],
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct CoreValueContainer {
    pub(crate) value: ValueId,
    pub(crate) kind: CoreContainerKind,
}

impl CoreValueContainer {
    pub(crate) fn flatten(self) -> Vec<ValueId> {
        match self.kind {
            CoreContainerKind::Raw { fat: Some(fat) } => vec![self.value, fat],
            _ => vec![self.value],
        }
    }
}

impl From<CoreValueContainer> for ValuePtrContainer {
    fn from(value: CoreValueContainer) -> Self {
        let kind = match value.kind {
            CoreContainerKind::Raw { fat } => ContainerKind::Raw { fat },
            CoreContainerKind::Ptr(ty) => ContainerKind::Ptr(ty),
        };
        Self {
            value_ptr: value.value,
            kind,
        }
    }
}

impl From<ValuePtrContainer> for CoreValueContainer {
    fn from(value: ValuePtrContainer) -> Self {
        let kind = match value.kind {
            ContainerKind::Raw { fat } => CoreContainerKind::Raw { fat },
            ContainerKind::Ptr(ty) => CoreContainerKind::Ptr(ty),
        };
        Self {
            value: value.value_ptr,
            kind,
        }
    }
}

#[derive(Debug, EnumAsInner, Clone)]
pub(crate) enum ContainerKind {
    Raw { fat: Option<ValueId> },
    Ptr(TypePtr),
}

#[derive(Debug, EnumAsInner, Clone)]
pub(crate) enum CoreContainerKind {
    Raw { fat: Option<ValueId> },
    Ptr(TypePtr),
}

#[derive(Debug, EnumAsInner)]
pub(crate) enum ValueKind {
    Normal(ValuePtrContainer),
    LenMethod(u32),
}

#[derive(Debug, EnumAsInner)]
pub(crate) enum CoreValueKind {
    Normal(CoreValueContainer),
    LenMethod(u32),
}

impl<'ast, 'analyzer> IRGenerator<'ast, 'analyzer> {
    pub(crate) fn get_value_type(&self, value: &ValuePtrContainer) -> TypePtr {
        match &value.kind {
            ContainerKind::Raw { fat: Some(..) } => self.fat_ptr_type().into(),
            ContainerKind::Raw { fat: None } => self.core_module.borrow().value_ty(value.value_ptr).clone(),
            ContainerKind::Ptr(ty) => ty.clone(),
        }
    }

    pub(crate) fn get_value_presentation(&mut self, value: ValuePtrContainer) -> ValuePtrContainer {
        match &value.kind {
            ContainerKind::Raw { .. } => match value.kind.clone() {
                ContainerKind::Raw { fat: Some(..) } => value,
                ContainerKind::Raw { fat: None } => {
                    let raw_ty = self.core_module.borrow().value_ty(value.value_ptr).clone();
                    if raw_ty.is_aggregate_type() {
                        self.get_value_ptr(value)
                    } else {
                        value
                    }
                }
                ContainerKind::Ptr(..) => unreachable!(),
            }
            ContainerKind::Ptr(ty) => {
                if ty.is_fat_ptr() {
                    ValuePtrContainer {
                        value_ptr: ValueId::Inst(self.core_builder.build_load(
                            self.context.ptr_type().into(),
                            value.value_ptr,
                            None,
                        )),
                        kind: ContainerKind::Raw {
                            fat: Some({
                                let zero = ValueId::Const(self.core_module.borrow_mut().add_i32_const(0));
                                let one = ValueId::Const(self.core_module.borrow_mut().add_i32_const(1));
                                let p = self.core_builder.build_getelementptr(
                                    self.fat_ptr_type().into(),
                                    value.value_ptr,
                                    vec![zero, one],
                                    None,
                                );
                                ValueId::Inst(self.core_builder.build_load(
                                    self.context.i32_type().into(),
                                    ValueId::Inst(p),
                                    None,
                                ))
                            }),
                        },
                    }
                } else if ty.is_aggregate_type() {
                    value
                } else {
                    ValuePtrContainer {
                        value_ptr: self.get_raw_value(value),
                        kind: ContainerKind::Raw { fat: None },
                    }
                }
            }
        }
    }

    pub(crate) fn raw_value_to_ptr(&mut self, raw: ValuePtrContainer) -> ValuePtrContainer {
        let raw_type = self.core_module.borrow().value_ty(raw.value_ptr).clone();
        if let Some(fat) = raw.kind.as_raw().unwrap() {
            debug_assert!(
                (raw_type.is_ptr() || raw_type.is_array())
                    && self.core_module.borrow().value_ty(*fat).is_int(),
                "raw: {:?}\nfat: {:?}",
                raw_type,
                fat
            );

            let fat_ptr_type = self.fat_ptr_type();
            let allocated = self.build_core_alloca(fat_ptr_type.clone().into(), None);
            let zero = ValueId::Const(self.core_module.borrow_mut().add_i32_const(0));
            let zero_idx = ValueId::Const(self.core_module.borrow_mut().add_i32_const(0));
            let head = if raw_type.is_ptr() {
                raw.value_ptr
            } else if raw_type.is_array() {
                ValueId::Inst(self.core_builder.build_getelementptr(
                    raw_type.clone(),
                    raw.value_ptr,
                    vec![zero, zero_idx],
                    None,
                ))
            } else {
                panic!("raw: {:?}\nfat: {:?}", raw_type, fat);
            };
            self.core_builder.build_store(head, allocated);
            let zero = ValueId::Const(self.core_module.borrow_mut().add_i32_const(0));
            let one = ValueId::Const(self.core_module.borrow_mut().add_i32_const(1));
            let second = self.core_builder.build_getelementptr(
                fat_ptr_type.clone().into(),
                allocated,
                vec![zero, one],
                None,
            );
            self.core_builder.build_store(*fat, ValueId::Inst(second));

            ValuePtrContainer {
                value_ptr: allocated,
                kind: ContainerKind::Ptr(fat_ptr_type.into()),
            }
        } else {
            let allocated = self.build_core_alloca(raw_type.clone(), None);
            self.core_builder.build_store(raw.value_ptr, allocated);
            ValuePtrContainer {
                value_ptr: allocated,
                kind: ContainerKind::Ptr(raw_type),
            }
        }
    }

    pub(crate) fn get_value_ptr(&mut self, value: ValuePtrContainer) -> ValuePtrContainer {
        match value.kind {
            ContainerKind::Raw { .. } => self.raw_value_to_ptr(value),
            ContainerKind::Ptr(..) => value,
        }
    }

    pub(crate) fn get_raw_value(&mut self, value: ValuePtrContainer) -> ValueId {
        match value.kind {
            ContainerKind::Raw { .. } => value.value_ptr,
            ContainerKind::Ptr(ty) => ValueId::Inst(self.core_builder.build_load(ty, value.value_ptr, None)),
        }
    }

    pub(crate) fn add_value_index(&mut self, index: ValueIndex, value: ValuePtrContainer) {
        self.value_indexes.insert(index, value);
    }

    pub(crate) fn add_core_value_index(&mut self, index: ValueIndex, value: CoreValueContainer) {
        self.core_value_indexes.insert(index, value);
    }

    pub(crate) fn get_value_by_index(&mut self, index: &ValueIndex) -> ValueKind {
        if let ValueIndex::Place(PlaceValueIndex {
            name,
            kind:
                ValueIndexKind::Impl {
                    ty:
                        ResolvedTy {
                            names: _,
                            kind: ResolvedTyKind::Array(_, len),
                        },
                    for_trait: None,
                },
        }) = index
            && name.0 == "len"
        {
            ValueKind::LenMethod(len.unwrap())
        } else {
            ValueKind::Normal(
                self.value_indexes
                    .get(index)
                    .unwrap_or_else(|| panic!("Can't get value by index: {:?}", index))
                    .clone(),
            )
        }
    }

    pub(crate) fn get_core_value_by_index(&mut self, index: &ValueIndex) -> CoreValueKind {
        self.try_get_core_value_by_index(index)
            .unwrap_or_else(|| panic!("Can't get core value by index: {:?}", index))
    }

    pub(crate) fn try_get_core_value_by_index(
        &self,
        index: &ValueIndex,
    ) -> Option<CoreValueKind> {
        if let ValueIndex::Place(PlaceValueIndex {
            name,
            kind:
                ValueIndexKind::Impl {
                    ty:
                        ResolvedTy {
                            names: _,
                            kind: ResolvedTyKind::Array(_, len),
                        },
                    for_trait: None,
                },
        }) = index
            && name.0 == "len"
        {
            Some(CoreValueKind::LenMethod(len.unwrap()))
        } else {
            self.core_value_indexes.get(index).cloned().map(CoreValueKind::Normal)
        }
    }

    pub(crate) fn store_to_ptr(&mut self, dest: ValueId, src: ValuePtrContainer) {
        match src.kind {
            ContainerKind::Raw { fat } => {
                self.core_builder.build_store(src.value_ptr, dest);
                if let Some(fat) = fat {
                    let zero = ValueId::Const(self.core_module.borrow_mut().add_i32_const(0));
                    let one = ValueId::Const(self.core_module.borrow_mut().add_i32_const(1));
                    let second = self.core_builder.build_getelementptr(
                        self.fat_ptr_type().into(),
                        dest,
                        vec![zero, one],
                        None,
                    );
                    self.core_builder.build_store(fat, ValueId::Inst(second));
                }
            }
            ContainerKind::Ptr(ty) => {
                self.core_builder.build_memcpy(dest, src.value_ptr, &ty);
            }
        };
    }

    pub(crate) fn core_get_value_type(&self, value: &CoreValueContainer) -> TypePtr {
        match &value.kind {
            CoreContainerKind::Raw { fat: Some(..) } => self.fat_ptr_type().into(),
            CoreContainerKind::Raw { fat: None } => self.core_module.borrow().value_ty(value.value).clone(),
            CoreContainerKind::Ptr(ty) => ty.clone(),
        }
    }

    pub(crate) fn core_get_value_presentation(
        &mut self,
        value: CoreValueContainer,
    ) -> CoreValueContainer {
        match &value.kind {
            CoreContainerKind::Raw { .. } => {
                match value.kind.clone() {
                    CoreContainerKind::Raw { fat: Some(..) } => value,
                    CoreContainerKind::Raw { fat: None } => {
                        let raw_ty = self.core_module.borrow().value_ty(value.value).clone();
                        if raw_ty.is_aggregate_type() {
                            self.core_get_value_ptr(value)
                        } else {
                            value
                        }
                    }
                    CoreContainerKind::Ptr(..) => unreachable!(),
                }
            }
            CoreContainerKind::Ptr(ty) => {
                if ty.is_fat_ptr() {
                    let value_ptr = value.value;
                    let head = self
                        .core_builder
                        .build_load(self.context.ptr_type().into(), value_ptr, None);
                    let zero = ValueId::Const(self.core_module.borrow_mut().add_i32_const(0));
                    let one = ValueId::Const(self.core_module.borrow_mut().add_i32_const(1));
                    let second_ptr = self.core_builder.build_getelementptr(
                        self.fat_ptr_type().into(),
                        value_ptr,
                        vec![zero, one],
                        None,
                    );
                    let fat = self
                        .core_builder
                        .build_load(self.context.i32_type().into(), ValueId::Inst(second_ptr), None);
                    CoreValueContainer {
                        value: ValueId::Inst(head),
                        kind: CoreContainerKind::Raw {
                            fat: Some(ValueId::Inst(fat)),
                        },
                    }
                } else if ty.is_aggregate_type() {
                    value
                } else {
                    CoreValueContainer {
                        value: self.core_get_raw_value(value),
                        kind: CoreContainerKind::Raw { fat: None },
                    }
                }
            }
        }
    }

    pub(crate) fn core_raw_value_to_ptr(&mut self, raw: CoreValueContainer) -> CoreValueContainer {
        let raw_type = self.core_module.borrow().value_ty(raw.value).clone();
        if let Some(fat) = raw.kind.as_raw().unwrap() {
            debug_assert!(
                (raw_type.is_ptr() || raw_type.is_array())
                    && self.core_module.borrow().value_ty(*fat).is_int(),
                "raw: {:?}\nfat: {:?}",
                raw_type,
                fat
            );

            let fat_ptr_type = self.fat_ptr_type();
            let allocated = self.build_core_alloca(fat_ptr_type.clone().into(), None);
            let zero = ValueId::Const(self.core_module.borrow_mut().add_i32_const(0));
            let zero_idx = ValueId::Const(self.core_module.borrow_mut().add_i32_const(0));
            let head = if raw_type.is_ptr() {
                raw.value
            } else if raw_type.is_array() {
                ValueId::Inst(self.core_builder.build_getelementptr(
                    raw_type.clone(),
                    raw.value,
                    vec![zero, zero_idx],
                    None,
                ))
            } else {
                panic!("raw: {:?}\nfat: {:?}", raw_type, fat);
            };
            self.core_builder.build_store(head, allocated);
            let zero = ValueId::Const(self.core_module.borrow_mut().add_i32_const(0));
            let one = ValueId::Const(self.core_module.borrow_mut().add_i32_const(1));
            let second = self.core_builder.build_getelementptr(
                fat_ptr_type.clone().into(),
                allocated,
                vec![zero, one],
                None,
            );
            self.core_builder.build_store(*fat, ValueId::Inst(second));

            CoreValueContainer {
                value: allocated,
                kind: CoreContainerKind::Ptr(fat_ptr_type.into()),
            }
        } else {
            let allocated = self.build_core_alloca(raw_type.clone(), None);
            self.core_builder.build_store(raw.value, allocated);
            CoreValueContainer {
                value: allocated,
                kind: CoreContainerKind::Ptr(raw_type),
            }
        }
    }

    pub(crate) fn core_get_value_ptr(&mut self, value: CoreValueContainer) -> CoreValueContainer {
        match value.kind {
            CoreContainerKind::Raw { .. } => self.core_raw_value_to_ptr(value),
            CoreContainerKind::Ptr(..) => value,
        }
    }

    pub(crate) fn core_get_raw_value(&mut self, value: CoreValueContainer) -> ValueId {
        match value.kind {
            CoreContainerKind::Raw { .. } => value.value,
            CoreContainerKind::Ptr(ty) => {
                ValueId::Inst(self.core_builder.build_load(ty, value.value, None))
            }
        }
    }

    pub(crate) fn core_store_to_ptr(&mut self, dest: ValueId, src: CoreValueContainer) {
        match src.kind {
            CoreContainerKind::Raw { fat } => {
                self.core_builder.build_store(src.value, dest);
                if let Some(fat) = fat {
                    let zero = ValueId::Const(self.core_module.borrow_mut().add_i32_const(0));
                    let one = ValueId::Const(self.core_module.borrow_mut().add_i32_const(1));
                    let second = self.core_builder.build_getelementptr(
                        self.fat_ptr_type().into(),
                        dest,
                        vec![zero, one],
                        None,
                    );
                    self.core_builder.build_store(fat, ValueId::Inst(second));
                }
            }
            CoreContainerKind::Ptr(ty) => {
                self.core_builder.build_memcpy(dest, src.value, &ty);
            }
        };
    }

    // let a: Struct;  PtrType, Ptr(Struct)
    // a.i32;          PtrType, Ptr(i32)
    // &a;             PtrType, Raw
    // (&a).i32;       PtrType, Ptr(i32)
    // let b = &a;     PtrType, Ptr(Ptr)
    // b.i32;
    pub(crate) fn deref_impl(
        &mut self,
        value: ValuePtrContainer,
        level: &DerefLevel,
        ty: &TypePtr,
        self_by_ref: bool,
    ) -> ValuePtrContainer {
        match level {
            DerefLevel::Not => {
                debug_assert!(!self_by_ref);
                value
            }
            DerefLevel::Deref(deref_level, ..) => {
                if self_by_ref {
                    return self.deref(value, deref_level, ty);
                }

                let value = self.get_raw_value(value);

                if deref_level.is_not() {
                    ValuePtrContainer {
                        value_ptr: value,
                        kind: ContainerKind::Ptr(ty.clone()),
                    }
                } else {
                    let new_value = self.core_builder.build_load(self.context.ptr_type().into(), value, None);
                    self.deref(
                        ValuePtrContainer {
                            value_ptr: ValueId::Inst(new_value),
                            kind: ContainerKind::Ptr(self.context.ptr_type().into()),
                        },
                        deref_level,
                        ty,
                    )
                }
            }
        }
    }

    pub(crate) fn deref(
        &mut self,
        value: ValuePtrContainer,
        level: &DerefLevel,
        ty: &TypePtr,
    ) -> ValuePtrContainer {
        self.deref_impl(value, level, ty, false)
    }

    pub(crate) fn core_deref_impl(
        &mut self,
        value: CoreValueContainer,
        level: &DerefLevel,
        ty: &TypePtr,
        self_by_ref: bool,
    ) -> CoreValueContainer {
        match level {
            DerefLevel::Not => {
                debug_assert!(!self_by_ref);
                value
            }
            DerefLevel::Deref(deref_level, ..) => {
                if self_by_ref {
                    return self.core_deref(value, deref_level, ty);
                }

                let value = self.core_get_raw_value(value);

                if deref_level.is_not() {
                    CoreValueContainer {
                        value,
                        kind: CoreContainerKind::Ptr(ty.clone()),
                    }
                } else {
                    let new_value =
                        self.core_builder
                            .build_load(self.context.ptr_type().into(), value, None);
                    self.core_deref(
                        CoreValueContainer {
                            value: ValueId::Inst(new_value),
                            kind: CoreContainerKind::Ptr(self.context.ptr_type().into()),
                        },
                        deref_level,
                        ty,
                    )
                }
            }
        }
    }

    pub(crate) fn core_deref(
        &mut self,
        value: CoreValueContainer,
        level: &DerefLevel,
        ty: &TypePtr,
    ) -> CoreValueContainer {
        self.core_deref_impl(value, level, ty, false)
    }
}
