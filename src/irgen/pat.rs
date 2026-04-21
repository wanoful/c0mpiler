use crate::{
    ast::{BindingMode, ByRef},
    irgen::{
        IRGenerator,
        extra::PatExtra,
        value::ValuePtrContainer,
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
            core_value: _,
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
            let ptr = self.build_core_alloca(self.context.ptr_type().into(), Some(&ident.symbol.0));
            let stored = self.get_value_ptr(right_ptr);
            self.core_builder.build_store(stored.value_ptr, ptr);
            ValuePtrContainer {
                value_ptr: ptr,
                kind: crate::irgen::value::ContainerKind::Ptr(self.context.ptr_type().into()),
            }
        } else if is_temp_value {
            self.get_value_ptr(right_ptr)
        } else {
            let ptr = self.build_core_alloca(ty.clone(), Some(&ident.symbol.0));
            self.store_to_ptr(ptr, right_ptr);

            ValuePtrContainer {
                value_ptr: ptr,
                kind: crate::irgen::value::ContainerKind::Ptr(ty.clone()),
            }
        };
        self.add_value_index(index.clone(), value.clone());
        self.add_core_value_index(index, value.into());
    }
}
