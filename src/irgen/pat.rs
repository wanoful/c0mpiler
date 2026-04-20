use crate::{
    ast::{BindingMode, ByRef},
    irgen::{
        IRGenerator,
        extra::PatExtra,
        value::{CoreContainerKind, CoreValueContainer, ValuePtrContainer},
    },
    semantics::value::{PlaceValueIndex, ValueIndex, ValueIndexKind},
};

impl<'ast, 'analyzer> IRGenerator<'ast, 'analyzer> {
    pub(crate) fn visit_ident_pat_impl(
        &mut self,
        BindingMode(by_ref, _): &BindingMode,
        ident: &crate::ast::Ident,
        PatExtra {
            value: right_ptr,
            core_value,
            self_id,
            is_temp_value,
        }: PatExtra,
    ) {
        let index = ValueIndex::Place(PlaceValueIndex {
            name: ident.symbol.clone(),
            kind: ValueIndexKind::Bindings {
                binding_id: self_id,
            },
        });

        let ty = self.get_value_type(&right_ptr);
        let value = if matches!(by_ref, ByRef::Yes(_)) {
            let ptr = self.build_alloca(self.context.ptr_type().into(), Some(&ident.symbol.0));
            self.builder
                .build_store(self.get_value_ptr(right_ptr).value_ptr, ptr.clone().into());
            ValuePtrContainer {
                value_ptr: ptr.into(),
                kind: crate::irgen::value::ContainerKind::Ptr(self.context.ptr_type().into()),
            }
        } else if is_temp_value {
            if right_ptr.value_ptr.get_name().is_none() {
                right_ptr.value_ptr.set_name(ident.symbol.0.clone());
            }
            self.get_value_ptr(right_ptr)
        } else {
            let ptr = self.build_alloca(ty.clone(), Some(&ident.symbol.0));
            self.store_to_ptr(ptr.clone().into(), right_ptr);

            ValuePtrContainer {
                value_ptr: ptr.into(),
                kind: crate::irgen::value::ContainerKind::Ptr(ty.clone()),
            }
        };
        self.add_value_index(index.clone(), value);

        if let Some(core_right) = core_value {
            let core_ty = self.core_get_value_type(&core_right);
            let core_value = if matches!(by_ref, ByRef::Yes(_)) {
                let ptr =
                    self.build_core_alloca(self.context.ptr_type().into(), Some(&ident.symbol.0));
                let stored = self.core_get_value_ptr(core_right);
                self.core_builder.build_store(stored.value, ptr);
                CoreValueContainer {
                    value: ptr,
                    kind: CoreContainerKind::Ptr(self.context.ptr_type().into()),
                }
            } else if is_temp_value {
                self.core_get_value_ptr(core_right)
            } else {
                let ptr = self.build_core_alloca(core_ty.clone(), Some(&ident.symbol.0));
                self.core_store_to_ptr(ptr, core_right);

                CoreValueContainer {
                    value: ptr,
                    kind: CoreContainerKind::Ptr(core_ty),
                }
            };
            self.add_core_value_index(index, core_value);
        }
    }
}
