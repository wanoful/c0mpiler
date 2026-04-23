use crate::{
    ast::{BindingMode, ByRef},
    irgen::{IRGenerator, extra::PatExtra, value::CoreValueContainer},
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
            let ptr = self.build_alloca(self.context.ptr_type().into(), Some(&ident.symbol.0));
            let stored = self.get_value_ptr(right_ptr);
            self.builder.build_store(stored.value, ptr);
            CoreValueContainer {
                value: ptr,
                kind: crate::irgen::value::CoreContainerKind::Ptr(self.context.ptr_type().into()),
            }
        } else if is_temp_value {
            self.get_value_ptr(right_ptr)
        } else {
            let ptr = self.build_alloca(ty.clone(), Some(&ident.symbol.0));
            self.store_to_ptr(ptr, right_ptr);

            CoreValueContainer {
                value: ptr,
                kind: crate::irgen::value::CoreContainerKind::Ptr(ty.clone()),
            }
        };
        self.add_value_index(index.clone(), value.clone());
        self.add_value_index(index, value);
    }
}
