pub mod expr;
pub mod extra;
pub mod pat;
pub mod ty;
pub mod value;
pub mod visitor;

use std::collections::{HashMap, HashSet};
use std::{
    cell::{Ref, RefCell},
    rc::Rc,
};

use crate::{
    ast::{Crate, NodeId, Symbol},
    impossible,
    ir::{
        LLVMContext,
        core::{ModuleCore, ValueId},
        core_builder::CursorBuilder,
        ir_type::TypePtr,
        layout::TargetDataLayout,
    },
    irgen::value::CoreValueContainer,
    semantics::{
        analyzer::SemanticAnalyzer,
        resolved_ty::{ResolvedTy, ResolvedTyKind, TypeKey},
        utils::FullName,
        value::{PlaceValue, PlaceValueIndex, ValueIndex},
        visitor::Visitor,
    },
};

pub struct IRGenerator<'ast, 'analyzer> {
    pub(crate) context: LLVMContext,
    pub(crate) module: Rc<RefCell<ModuleCore>>,
    pub(crate) builder: CursorBuilder,
    pub(crate) alloca_builder: CursorBuilder,

    pub(crate) analyzer: &'analyzer SemanticAnalyzer<'ast>,

    pub(crate) value_indexes: HashMap<ValueIndex, CoreValueContainer>,
    pub(crate) expr_values: HashMap<NodeId, CoreValueContainer>,

    pub(crate) visited_impls: HashSet<(TypeKey, Option<TypeKey>)>,
}

impl<'ast, 'analyzer> IRGenerator<'ast, 'analyzer> {
    pub fn new(analyzer: &'analyzer SemanticAnalyzer<'ast>, target: TargetDataLayout) -> Self {
        let context = LLVMContext::new(target);
        let core_module = Rc::new(RefCell::new(ModuleCore::new()));
        core_module.borrow_mut().set_target_data_layout(target);
        let mut core_builder = CursorBuilder::new(core_module.clone());
        let core_alloca_builder = CursorBuilder::new(core_module.clone());
        let mut core_value_indexes = HashMap::default();
        let core_expr_values = HashMap::default();

        add_builtin_struct_types(&context);
        add_preludes(&context, &mut core_builder, &mut core_value_indexes);
        let value_indexes = core_value_indexes
            .iter()
            .map(|(index, value)| (index.clone(), value.clone()))
            .collect();

        let mut generator = Self {
            context,
            module: core_module,
            builder: core_builder,
            alloca_builder: core_alloca_builder,
            analyzer,
            value_indexes,
            expr_values: core_expr_values,
            visited_impls: HashSet::new(),
        };

        generator.add_struct_type();
        generator.sync_named_structs_to_core();
        generator.absorb_analyzer_global_values(0);
        generator.absorb_analyzer_methods();

        generator
    }

    pub fn visit(&mut self, krate: &'ast Crate) {
        self.visit_crate(krate, ());
    }

    pub fn print(&self) -> String {
        self.module.borrow().print()
    }

    pub fn core_print(&self) -> String {
        self.module.borrow().print()
    }

    pub fn module(&self) -> Ref<'_, ModuleCore> {
        self.module.borrow()
    }

    fn absorb_analyzer_struct(&self, structs: &mut HashMap<TypeKey, ResolvedTy>) {
        self.absorb_scope_struct(0, structs);
    }

    fn absorb_scope_struct(&self, scope_id: NodeId, structs: &mut HashMap<TypeKey, ResolvedTy>) {
        let scope = self.analyzer.get_scope(scope_id);
        for key in scope.types.values() {
            let ty = self.analyzer.probe_type((*key).into()).unwrap();

            use crate::semantics::resolved_ty::ResolvedTyKind::*;
            if let Tup(_) = ty.kind {
                structs.insert(*key, ty);
            }
        }

        for child in &scope.children {
            self.absorb_scope_struct(*child, structs);
        }
    }

    fn add_struct_type(&mut self) {
        let mut map = HashMap::new();
        self.absorb_analyzer_struct(&mut map);

        for ty in map.values() {
            self.context
                .create_opaque_struct_type(&ty.names.as_ref().unwrap().0.to_string());
        }

        for ty in map.values() {
            let struct_ty = self
                .context
                .get_named_struct_type(&ty.names.as_ref().unwrap().0.to_string())
                .unwrap();
            struct_ty.set_body(
                ty.kind
                    .as_tup()
                    .unwrap()
                    .iter()
                    .map(|x| self.transform_interned_ty_faithfully(*x))
                    .collect(),
                false,
            );
        }
    }

    fn sync_named_structs_to_core(&mut self) {
        self.module
            .borrow_mut()
            .extend_named_structs(self.context.named_struct_types());
    }

    fn absorb_analyzer_global_values(&mut self, scope_id: NodeId) {
        let scope = self.analyzer.get_scope(scope_id);
        for (s, PlaceValue { value, .. }) in &scope.values {
            if s.0 == "exit" && scope_id == 0 {
                continue;
            }
            let is_main_function = self.analyzer.is_main_function(s, scope_id);
            let full_name = self.analyzer.get_full_name(scope_id, s.clone());

            let core_v = self.absorb_analyzer_global_value_core(value, is_main_function, full_name);
            let v: CoreValueContainer = core_v.clone();
            let index = ValueIndex::Place(PlaceValueIndex {
                name: s.clone(),
                kind: crate::semantics::value::ValueIndexKind::Global { scope_id },
            });
            self.value_indexes.insert(index.clone(), v);
        }

        for id in &scope.children {
            self.absorb_analyzer_global_values(*id);
        }
    }

    fn absorb_analyzer_global_value_core(
        &mut self,
        value: &crate::semantics::value::Value<'ast>,
        is_main_function: bool,
        full_name: FullName,
    ) -> CoreValueContainer {
        use crate::semantics::value::ValueKind::*;

        match &value.kind {
            Constant(inner) => {
                let init = self.create_constant_initialization_core(inner, value.ty);
                let ty = self.module.borrow().const_data(init).ty.clone();
                let global = self.module.borrow_mut().add_global(
                    full_name.to_string(),
                    ty.clone(),
                    crate::ir::core_value::GlobalKind::GlobalVariable {
                        is_constant: true,
                        initializer: Some(init),
                    },
                );

                CoreValueContainer {
                    value: ValueId::Global(global),
                    kind: crate::irgen::value::CoreContainerKind::Ptr(ty),
                }
            }
            Fn {
                is_placeholder,
                ast_node,
                ..
            } => {
                let mut fn_resolved_ty = self.analyzer.probe_type(value.ty).unwrap();

                if is_main_function {
                    *fn_resolved_ty.kind.as_fn_mut().unwrap().0 = self.analyzer.i32_type();
                }

                let (ret_intern, arg_interns) = fn_resolved_ty.kind.as_fn_mut().unwrap();

                let mut ret_ty = self.transform_interned_ty_faithfully(*ret_intern);
                let mut arg_tys = Vec::new();

                let i32_type = self.context.i32_type();
                let ptr_type = self.context.ptr_type();

                for arg_intern in arg_interns {
                    let arg_ty = self.transform_interned_ty_impl(
                        *arg_intern,
                        ty::TransformTypeConfig::FirstClass,
                    );
                    if let Some(s) = arg_ty.as_struct()
                        && let Some(name) = s.get_name()
                        && name == "fat_ptr"
                    {
                        arg_tys.push(ptr_type.clone().into());
                        arg_tys.push(i32_type.clone().into());
                    } else {
                        arg_tys.push(arg_ty);
                    }
                }

                let mut aggregate_type = None;
                if ret_ty.is_zero_length_type() {
                    ret_ty = self.context.void_type().into();
                } else if ret_ty.is_aggregate_type() {
                    arg_tys.insert(0, self.context.ptr_type().into());
                    aggregate_type = Some(ret_ty);
                    ret_ty = self.context.void_type().into();
                }

                let fn_ty = self.context.function_type(ret_ty.clone(), arg_tys);
                let is_declare = *is_placeholder
                    || matches!(ast_node, crate::semantics::value::FnAstRefInfo::None);
                let func = if is_declare {
                    self.module
                        .borrow_mut()
                        .declare_function_value(full_name.to_string(), fn_ty.clone())
                } else {
                    self.module
                        .borrow_mut()
                        .define_function_value(full_name.to_string(), fn_ty.clone())
                };
                {
                    let mut module = self.module.borrow_mut();
                    module.append_signature_args(func);
                    if let Some(aggregate_type) = aggregate_type {
                        module.set_sret(func, aggregate_type);
                    }
                }
                let func_value = self
                    .module
                    .borrow()
                    .get_function_value(func)
                    .expect("function should have a value global");

                CoreValueContainer {
                    value: ValueId::Global(func_value),
                    kind: crate::irgen::value::CoreContainerKind::Ptr(fn_ty.into()),
                }
            }
            _ => impossible!(),
        }
    }

    fn create_constant_initialization_core(
        &mut self,
        value: &crate::semantics::value::ConstantValue<'_>,
        intern: crate::semantics::resolved_ty::TypeIntern,
    ) -> crate::ir::core::ConstId {
        let probe = self.analyzer.probe_type(intern).unwrap();
        use crate::semantics::value::ConstantValue::*;
        match value {
            ConstantInt(i) => {
                use crate::semantics::resolved_ty::BuiltInTyKind::*;

                match probe.kind {
                    ResolvedTyKind::BuiltIn(builtin) => match builtin {
                        Bool => self.module.borrow_mut().add_i1_const(*i != 0),
                        Char => self.module.borrow_mut().add_int_const(8, *i as i64),
                        I32 | ISize | U32 | USize => self.module.borrow_mut().add_i32_const(*i),
                        Str => impossible!(),
                    },
                    ResolvedTyKind::Enum => self.module.borrow_mut().add_i32_const(*i),
                    _ => impossible!(),
                }
            }
            ConstantString(string) => self.module.borrow_mut().add_string_const(string.clone()),
            ConstantArray(inners) => {
                let inner_ty = probe.kind.as_array().unwrap().0;
                let values = inners
                    .iter()
                    .map(|x| self.create_constant_initialization_core(x, *inner_ty))
                    .collect();
                let ty = self.transform_interned_ty_faithfully(intern);
                self.module.borrow_mut().add_array_const(ty, values)
            }
            Unit | UnitStruct => {
                let ty = self.transform_interned_ty_faithfully(intern);
                self.module.borrow_mut().add_struct_const(ty, vec![])
            }
            UnEval(_) | Placeholder => impossible!(),
        }
    }

    fn absorb_analyzer_methods(&mut self) {
        for (resolved_ty, impls) in &self.analyzer.impls {
            if resolved_ty.kind.is_trait() {
                continue;
            }
            let Some((name, _)) = &resolved_ty.names else {
                continue;
            };
            if *name == FullName(vec!["String".into()]) {
                continue;
            }
            for (s, PlaceValue { value, .. }) in &impls.inherent.values {
                let full_name = name.clone().concat(s.clone());
                let core_v = self.absorb_analyzer_global_value_core(value, false, full_name);
                let v: CoreValueContainer = core_v.clone();
                let index = ValueIndex::Place(PlaceValueIndex {
                    name: s.clone(),
                    kind: crate::semantics::value::ValueIndexKind::Impl {
                        ty: resolved_ty.clone(),
                        for_trait: None,
                    },
                });
                self.value_indexes.insert(index.clone(), v);
            }
            for (trait_ty, impl_info) in &impls.traits {
                let name = name
                    .clone()
                    .append(trait_ty.names.as_ref().unwrap().0.clone());
                for (s, PlaceValue { value, .. }) in &impl_info.values {
                    let full_name = name.clone().concat(s.clone());
                    let core_v = self.absorb_analyzer_global_value_core(value, false, full_name);
                    let v: CoreValueContainer = core_v.clone();
                    let index = ValueIndex::Place(PlaceValueIndex {
                        name: s.clone(),
                        kind: crate::semantics::value::ValueIndexKind::Impl {
                            ty: resolved_ty.clone(),
                            for_trait: Some(trait_ty.clone()),
                        },
                    });
                    self.value_indexes.insert(index.clone(), v);
                }
            }
        }
    }

    pub fn build_alloca(&mut self, ty: TypePtr, name: Option<&str>) -> ValueId {
        ValueId::Inst(self.alloca_builder.build_alloca(ty, name))
    }

    pub(crate) fn set_expr_value(&mut self, expr_id: NodeId, value: CoreValueContainer) {
        self.expr_values.insert(expr_id, value);
    }

    pub(crate) fn get_expr_value(&self, expr_id: &NodeId) -> Option<CoreValueContainer> {
        self.expr_values.get(expr_id).cloned()
    }

    pub(crate) fn clear_expr_value(&mut self, expr_id: &NodeId) {
        self.expr_values.remove(expr_id);
    }

    pub fn opt_mem2reg(&mut self) {
        self.module.borrow_mut().opt_pass_mem2reg();
    }

    pub fn opt_dce(&mut self) {
        self.module.borrow_mut().opt_dead_code_elimination();
    }

    pub fn opt_adce(&mut self) {
        self.module
            .borrow_mut()
            .opt_aggressive_dead_code_elimination();
    }

    pub fn opt_sccp(&mut self) {
        self.module
            .borrow_mut()
            .opt_sparse_conditional_constant_propagation();
    }

    pub fn opt_cfg_simplify(&mut self) {
        self.module.borrow_mut().opt_cfg_simplify();
    }

    pub fn opt_merge_return(&mut self) {
        self.module.borrow_mut().opt_merge_return();
    }
}

fn add_builtin_struct_types(context: &LLVMContext) {
    let fat_ptr = context
        .get_named_struct_type("fat_ptr")
        .unwrap_or_else(|| context.create_opaque_struct_type("fat_ptr"));
    fat_ptr.set_body(
        vec![context.ptr_type().into(), context.i32_type().into()],
        false,
    );

    let string = context
        .get_named_struct_type("String")
        .unwrap_or_else(|| context.create_opaque_struct_type("String"));
    string.set_body(
        vec![context.ptr_type().into(), context.i32_type().into()],
        false,
    );
}

fn add_preludes(
    context: &LLVMContext,
    builder: &mut CursorBuilder,
    value_indexes: &mut HashMap<ValueIndex, CoreValueContainer>,
) {
    let string_type: TypePtr = context.get_named_struct_type("String").unwrap().into();
    let fat_ptr_type: TypePtr = context.get_named_struct_type("fat_ptr").unwrap().into();

    let str_len_type = context.function_type(
        context.i32_type().into(),
        vec![context.ptr_type().into(), context.i32_type().into()],
    );
    let str_len_fn = builder
        .module()
        .borrow_mut()
        .create_function_value("str.len".to_string(), str_len_type.clone());
    let str_len_args = builder
        .module()
        .borrow_mut()
        .append_signature_args(str_len_fn);
    let str_len_entry = builder.module().borrow().entry_block(str_len_fn).unwrap();
    builder.locate_end(str_len_fn, str_len_entry);
    builder.build_return(Some(ValueId::Arg(str_len_args[1])));
    value_indexes.insert(
        ValueIndex::Place(PlaceValueIndex {
            name: "len".into(),
            kind: crate::semantics::value::ValueIndexKind::Impl {
                ty: ResolvedTy {
                    names: None,
                    kind: crate::semantics::resolved_ty::ResolvedTyKind::BuiltIn(
                        crate::semantics::resolved_ty::BuiltInTyKind::Str,
                    ),
                },
                for_trait: None,
            },
        }),
        CoreValueContainer {
            value: builder
                .get_function_value(str_len_fn)
                .expect("prelude function should have a global value"),
            kind: crate::irgen::value::CoreContainerKind::Ptr(str_len_type.into()),
        },
    );

    let string_as_str_type = context.function_type(
        context.void_type().into(),
        vec![context.ptr_type().into(), context.ptr_type().into()],
    );
    let string_as_str_fn = builder
        .module()
        .borrow_mut()
        .declare_function_value("string_as_str".to_string(), string_as_str_type.clone());
    {
        let module_handle = builder.module();
        let mut module = module_handle.borrow_mut();
        module.append_signature_args(string_as_str_fn);
        module.set_sret(string_as_str_fn, fat_ptr_type);
    }
    let string_index = crate::semantics::value::ValueIndexKind::Impl {
        ty: ResolvedTy {
            names: Some((FullName(vec![Symbol::from("String")]), None)),
            kind: ResolvedTyKind::Placeholder,
        },
        for_trait: None,
    };
    value_indexes.insert(
        ValueIndex::Place(PlaceValueIndex {
            name: "as_str".into(),
            kind: string_index.clone(),
        }),
        CoreValueContainer {
            value: builder
                .get_function_value(string_as_str_fn)
                .expect("prelude function should have a global value"),
            kind: crate::irgen::value::CoreContainerKind::Ptr(string_as_str_type.into()),
        },
    );

    let string_len_type =
        context.function_type(context.i32_type().into(), vec![context.ptr_type().into()]);
    let string_len_fn = builder
        .module()
        .borrow_mut()
        .declare_function_value("string_len".to_string(), string_len_type.clone());
    builder
        .module()
        .borrow_mut()
        .append_signature_args(string_len_fn);
    value_indexes.insert(
        ValueIndex::Place(PlaceValueIndex {
            name: "len".into(),
            kind: string_index.clone(),
        }),
        CoreValueContainer {
            value: builder
                .get_function_value(string_len_fn)
                .expect("prelude function should have a global value"),
            kind: crate::irgen::value::CoreContainerKind::Ptr(string_len_type.into()),
        },
    );

    let i32_to_string_type = context.function_type(
        context.void_type().into(),
        vec![context.ptr_type().into(), context.ptr_type().into()],
    );
    let to_string_fn = builder
        .module()
        .borrow_mut()
        .declare_function_value("to_string".to_string(), i32_to_string_type.clone());
    {
        let module_handle = builder.module();
        let mut module = module_handle.borrow_mut();
        module.append_signature_args(to_string_fn);
        module.set_sret(to_string_fn, string_type.clone());
    }
    value_indexes.insert(
        ValueIndex::Place(PlaceValueIndex {
            name: "to_string".into(),
            kind: crate::semantics::value::ValueIndexKind::Impl {
                ty: ResolvedTy {
                    names: None,
                    kind: crate::semantics::resolved_ty::ResolvedTyKind::BuiltIn(
                        crate::semantics::resolved_ty::BuiltInTyKind::U32,
                    ),
                },
                for_trait: None,
            },
        }),
        CoreValueContainer {
            value: builder
                .get_function_value(to_string_fn)
                .expect("prelude function should have a global value"),
            kind: crate::irgen::value::CoreContainerKind::Ptr(i32_to_string_type.clone().into()),
        },
    );
    value_indexes.insert(
        ValueIndex::Place(PlaceValueIndex {
            name: "to_string".into(),
            kind: crate::semantics::value::ValueIndexKind::Impl {
                ty: ResolvedTy {
                    names: None,
                    kind: crate::semantics::resolved_ty::ResolvedTyKind::BuiltIn(
                        crate::semantics::resolved_ty::BuiltInTyKind::USize,
                    ),
                },
                for_trait: None,
            },
        }),
        CoreValueContainer {
            value: builder
                .get_function_value(to_string_fn)
                .expect("prelude function should have a global value"),
            kind: crate::irgen::value::CoreContainerKind::Ptr(i32_to_string_type.into()),
        },
    );

    let string_plus_type = context.function_type(
        context.void_type().into(),
        vec![
            context.ptr_type().into(),
            context.ptr_type().into(),
            context.ptr_type().into(),
            context.i32_type().into(),
        ],
    );
    let string_plus_fn = builder
        .module()
        .borrow_mut()
        .declare_function_value("string_plus".to_string(), string_plus_type.clone());
    {
        let module_handle = builder.module();
        let mut module = module_handle.borrow_mut();
        module.append_signature_args(string_plus_fn);
        module.set_sret(string_plus_fn, string_type);
    }
    value_indexes.insert(
        ValueIndex::Place(PlaceValueIndex {
            name: "string_plus".into(),
            kind: crate::semantics::value::ValueIndexKind::Global { scope_id: 0 },
        }),
        CoreValueContainer {
            value: builder
                .get_function_value(string_plus_fn)
                .expect("prelude function should have a global value"),
            kind: crate::irgen::value::CoreContainerKind::Ptr(string_plus_type.into()),
        },
    );
}
