use std::iter::{once, zip};

use crate::{
    ast::{BindingMode, Crate, Mutability, expr::*, item::*, pat::*, stmt::*},
    impossible,
    ir::{core::ValueId, ir_type::ArrayType},
    irgen::{
        IRGenerator,
        extra::{CoreCycleInfo, ExprExtra, ItemExtra, PatExtra},
        ty::TransformTypeConfig,
        value::{
            ContainerKind, CoreContainerKind, CoreValueContainer, ValueKind, ValuePtrContainer,
        },
    },
    semantics::{
        item::AssociatedInfo,
        resolved_ty::ResolvedTy,
        value::{FnAstRefInfo, PlaceValueIndex, ValueIndex, ValueIndexKind},
        visitor::Visitor,
    },
};

impl<'ast, 'analyzer> IRGenerator<'ast, 'analyzer> {
    pub(crate) fn legacy_non_void_value(
        &self,
        value: Option<ValuePtrContainer>,
    ) -> Option<ValuePtrContainer> {
        value.filter(|value| !self.get_value_type(value).is_void())
    }

    pub(crate) fn core_non_void_value(
        &self,
        value: Option<CoreValueContainer>,
    ) -> Option<CoreValueContainer> {
        value.filter(|value| !self.core_module.borrow().value_ty(value.value).is_void())
    }

    pub(crate) fn core_branch_value(&self, expr: &'ast Expr) -> Option<CoreValueContainer> {
        self.get_core_expr_value(&expr.id)
            .or_else(|| match &expr.kind {
                ExprKind::Block(BlockExpr { stmts, .. }) => stmts.iter().rev().find_map(|stmt| {
                    if let StmtKind::Expr(inner) = &stmt.kind {
                        self.get_core_expr_value(&inner.id)
                    } else {
                        None
                    }
                }),
                _ => None,
            })
    }

    pub(crate) fn core_block_value(&self, block: &'ast BlockExpr) -> Option<CoreValueContainer> {
        self.get_core_expr_value(&block.id).or_else(|| {
            block.stmts.iter().rev().find_map(|stmt| {
                if let StmtKind::Expr(inner) = &stmt.kind {
                    self.get_core_expr_value(&inner.id)
                } else {
                    None
                }
            })
        })
    }
}

impl<'ast, 'analyzer> Visitor<'ast> for IRGenerator<'ast, 'analyzer> {
    type DefaultRes<'res>
        = ()
    where
        Self: 'res;

    type ExprRes<'res>
        = Option<ValuePtrContainer>
    where
        Self: 'res;

    type PatRes<'res>
        = ()
    where
        Self: 'res;

    type StmtRes<'res>
        = Option<ValuePtrContainer>
    where
        Self: 'res;

    type CrateExtra<'tmp> = ();

    type ItemExtra<'tmp> = ItemExtra;

    type StmtExtra<'tmp> = ExprExtra<'tmp>;

    type ExprExtra<'tmp> = ExprExtra<'tmp>;

    type PatExtra<'tmp> = PatExtra;

    fn visit_crate<'tmp>(
        &mut self,
        Crate { items, id }: &'ast crate::ast::Crate,
        _extra: Self::CrateExtra<'tmp>,
    ) -> Self::DefaultRes<'_> {
        for item in items {
            self.visit_item(
                item,
                ItemExtra {
                    scope_id: *id,
                    self_id: 0,
                    associated_info: None,
                },
            );
        }
    }

    fn visit_item<'tmp>(
        &mut self,
        Item { kind, id, span: _ }: &'ast Item,
        extra: Self::ItemExtra<'tmp>,
    ) -> Self::DefaultRes<'_> {
        let new_extra = ItemExtra {
            self_id: *id,
            ..extra
        };

        match kind {
            ItemKind::Const(const_item) => self.visit_const_item(const_item, new_extra),
            ItemKind::Fn(fn_item) => self.visit_fn_item(fn_item, new_extra),
            ItemKind::Mod(mod_item) => self.visit_mod_item(mod_item, new_extra),
            ItemKind::Enum(enum_item) => self.visit_enum_item(enum_item, new_extra),
            ItemKind::Struct(struct_item) => self.visit_struct_item(struct_item, new_extra),
            ItemKind::Trait(trait_item) => self.visit_trait_item(trait_item, new_extra),
            ItemKind::Impl(impl_item) => self.visit_impl_item(impl_item, new_extra),
        }
    }

    fn visit_const_item<'tmp>(
        &mut self,
        _item: &'ast ConstItem,
        _extra: Self::ItemExtra<'tmp>,
    ) -> Self::DefaultRes<'_> {
    }

    fn visit_fn_item<'tmp>(
        &mut self,
        FnItem {
            ident,
            generics: _,
            sig: FnSig { decl, span: _ },
            body,
        }: &'ast FnItem,
        extra: Self::ItemExtra<'tmp>,
    ) -> Self::DefaultRes<'_> {
        let (fn_value, full_name) = match extra.associated_info {
            Some(info) => {
                let instance = self.analyzer.probe_type(info.ty.into()).unwrap();
                let mut full_name = instance.names.unwrap().0;
                if let Some(trait_intern) = info.for_trait {
                    let trait_ty = self.analyzer.probe_type(trait_intern.into()).unwrap();
                    full_name = full_name.append(trait_ty.names.unwrap().0);
                }
                let full_name = full_name.concat(ident.symbol.clone());
                (
                    self.analyzer.get_impl_value(&info, &ident.symbol).unwrap(),
                    full_name,
                )
            }
            None => (
                self.analyzer
                    .get_scope_value(extra.scope_id, &ident.symbol)
                    .unwrap(),
                self.analyzer
                    .get_full_name(extra.scope_id, ident.symbol.clone()),
            ),
        };

        let name_string = full_name.to_string();
        let core_fn = self
            .core_module
            .borrow()
            .get_function(&name_string)
            .expect(&name_string);
        let fn_intern = fn_value.value.ty;
        let fn_resolved_ty = self.analyzer.probe_type(fn_intern).unwrap();

        let arg_types = fn_resolved_ty
            .kind
            .as_fn()
            .unwrap()
            .1
            .iter()
            .map(|x| self.transform_interned_ty_impl(*x, TransformTypeConfig::Faithful))
            .collect::<Vec<_>>();

        let core_loc = self.core_builder.get_location();
        let core_alloca_loc = self.core_alloca_builder.get_location();

        let core_bb = self.core_builder.append_block(core_fn, Some("entry"));
        {
            let mut module = self.core_module.borrow_mut();
            module.func_mut(core_fn).entry = core_bb.block;
        }
        self.core_builder.locate_end(core_fn, core_bb);
        self.core_alloca_builder.locate_front(core_fn, core_bb);

        let core_args = self.core_module.borrow().args_in_order(core_fn);
        let core_sret = self.core_module.borrow().sret_type(core_fn);
        let (core_ret_arg, core_input_args) = if core_sret.is_some() {
            let (ret, args) = core_args.split_first().unwrap();
            (Some(*ret), args)
        } else {
            (None, core_args.as_slice())
        };

        let mut core_args_iter = core_input_args.iter();
        for (arg_type, param) in zip(arg_types, &decl.inputs) {
            let core_arg = *core_args_iter.next().unwrap();
            let kind = if arg_type.is_aggregate_type() {
                ContainerKind::Ptr(arg_type.clone())
            } else {
                ContainerKind::Raw {
                    fat: if arg_type.is_fat_ptr() {
                        Some(ValueId::Arg(*core_args_iter.next().unwrap()))
                    } else {
                        None
                    },
                }
            };
            self.visit_pat(
                &param.pat,
                PatExtra {
                    value: ValuePtrContainer {
                        value_ptr: ValueId::Arg(core_arg),
                        kind: kind.clone(),
                    },
                    core_value: Some(CoreValueContainer {
                        value: ValueId::Arg(core_arg),
                        kind: match kind {
                            ContainerKind::Raw { fat } => CoreContainerKind::Raw { fat },
                            ContainerKind::Ptr(ty) => CoreContainerKind::Ptr(ty),
                        },
                    }),
                    self_id: 0,
                    is_temp_value: false,
                },
            );
        }

        let self_ty = extra
            .associated_info
            .map(|info| self.analyzer.probe_type_instance(info.ty.into()).unwrap());
        let value = self.visit_block_expr(
            body.as_ref().unwrap(),
            ExprExtra {
                scope_id: extra.self_id,
                self_id: body.as_ref().unwrap().id,
                core_cycle_info: None,
                core_ret_ptr: core_ret_arg.map(ValueId::Arg),
                self_ty: self_ty.as_ref(),
            },
        );

        if let Some(value) = value {
            if let Some(core_ret_ptr) = core_ret_arg.map(ValueId::Arg) {
                self.store_to_ptr(core_ret_ptr, value);
                self.core_builder.build_return(None);
            } else {
                let raw = self.get_raw_value(value);
                self.core_builder.build_return(Some(raw));
            }
        } else {
            self.try_build_return(None, &body.as_ref().unwrap().id);
        }

        self.core_builder.set_location(core_loc);
        self.core_alloca_builder.set_location(core_alloca_loc);
    }

    fn visit_mod_item<'tmp>(
        &mut self,
        _item: &'ast ModItem,
        _extra: Self::ItemExtra<'tmp>,
    ) -> Self::DefaultRes<'_> {
        impossible!()
    }

    fn visit_enum_item<'tmp>(
        &mut self,
        _item: &'ast EnumItem,
        _extra: Self::ItemExtra<'tmp>,
    ) -> Self::DefaultRes<'_> {
    }

    fn visit_struct_item<'tmp>(
        &mut self,
        _item: &'ast StructItem,
        _extra: Self::ItemExtra<'tmp>,
    ) -> Self::DefaultRes<'_> {
    }

    fn visit_trait_item<'tmp>(
        &mut self,
        _item: &'ast TraitItem,
        _extra: Self::ItemExtra<'tmp>,
    ) -> Self::DefaultRes<'_> {
    }

    fn visit_impl_item<'tmp>(
        &mut self,
        ImplItem { .. }: &'ast ImplItem,
        extra: Self::ItemExtra<'tmp>,
    ) -> Self::DefaultRes<'_> {
        let scope = self.analyzer.get_scope(extra.self_id);
        let (ty, for_trait) = scope.kind.as_impl().unwrap();

        // TODO: 遍历 impl 应该可以改为不依赖 ast 树，而不是使用这个 hack
        if !self.visited_impls.insert((*ty, *for_trait)) {
            return;
        }

        let impls = self.analyzer.get_impls(ty).unwrap();
        let impl_info = if let Some(trait_intern) = for_trait {
            let trait_instance = self
                .analyzer
                .probe_type_instance((*trait_intern).into())
                .unwrap();
            impls.traits.get(&trait_instance).unwrap()
        } else {
            &impls.inherent
        };
        let asso_info = AssociatedInfo {
            is_trait: false,
            ty: *ty,
            for_trait: *for_trait,
        };
        for v in impl_info.values.values() {
            if let Some((
                _,
                _,
                FnAstRefInfo::Inherent(ast_fn, node_id) | FnAstRefInfo::Trait(ast_fn, node_id),
            )) = v.value.kind.as_fn()
            {
                self.visit_fn_item(
                    ast_fn,
                    ItemExtra {
                        scope_id: extra.scope_id,
                        self_id: *node_id,
                        associated_info: Some(asso_info),
                    },
                )
            }
        }
    }

    fn visit_associate_item<'tmp>(
        &mut self,
        _: &'ast crate::ast::item::Item<crate::ast::item::AssocItemKind>,
        _extra: Self::ItemExtra<'tmp>,
    ) -> Self::DefaultRes<'_> {
    }

    fn visit_stmt<'tmp>(
        &mut self,
        Stmt { kind, id, span: _ }: &'ast Stmt,
        extra: Self::StmtExtra<'tmp>,
    ) -> Self::StmtRes<'_> {
        let new_extra = ExprExtra {
            self_id: *id,
            ..extra
        };
        match &kind {
            StmtKind::Let(local_stmt) => self.visit_local_stmt(local_stmt, new_extra),
            StmtKind::Item(item) => {
                self.visit_item(
                    item,
                    ItemExtra {
                        scope_id: extra.scope_id,
                        self_id: extra.self_id,
                        associated_info: None,
                    },
                );
                None
            }
            StmtKind::Expr(expr) => self.visit_expr(expr, new_extra),
            StmtKind::Semi(expr) => {
                self.visit_expr(expr, new_extra);
                None
            }
            StmtKind::Empty(_) => None,
        }
    }

    fn visit_local_stmt<'tmp>(
        &mut self,
        LocalStmt {
            pat,
            ty: _,
            kind,
            id: _,
            span: _,
        }: &'ast LocalStmt,
        extra: Self::StmtExtra<'tmp>,
    ) -> Self::StmtRes<'_> {
        let (value, is_temp_value) = match kind {
            LocalKind::Decl => impossible!(),
            LocalKind::Init(expr) => (
                self.visit_expr(expr, extra)?,
                matches!(
                    self.analyzer.get_expr_result(&expr.id).assignee,
                    crate::semantics::expr::AssigneeKind::Value
                ),
            ),
        };
        let core_value = match kind {
            LocalKind::Decl => None,
            LocalKind::Init(expr) => self.core_branch_value(expr),
        };

        self.visit_pat(
            pat,
            PatExtra {
                value,
                core_value,
                self_id: 0,
                is_temp_value,
            },
        );

        None
    }

    fn visit_expr<'tmp>(
        &mut self,
        Expr { kind, span: _, id }: &'ast Expr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        let new_extra = ExprExtra {
            self_id: *id,
            ..extra
        };
        match kind {
            ExprKind::Array(expr) => self.visit_array_expr(expr, new_extra),
            ExprKind::ConstBlock(expr) => self.visit_const_block_expr(expr, new_extra),
            ExprKind::Call(expr) => self.visit_call_expr(expr, new_extra),
            ExprKind::MethodCall(expr) => self.visit_method_call_expr(expr, new_extra),
            ExprKind::Tup(expr) => self.visit_tup_expr(expr, new_extra),
            ExprKind::Binary(expr) => self.visit_binary_expr(expr, new_extra),
            ExprKind::Unary(expr) => self.visit_unary_expr(expr, new_extra),
            ExprKind::Lit(expr) => self.visit_lit_expr(expr, new_extra),
            ExprKind::Cast(expr) => self.visit_cast_expr(expr, new_extra),
            ExprKind::Let(expr) => self.visit_let_expr(expr, new_extra),
            ExprKind::If(expr) => self.visit_if_expr(expr, new_extra),
            ExprKind::While(expr) => self.visit_while_expr(expr, new_extra),
            ExprKind::ForLoop(expr) => self.visit_for_loop_expr(expr, new_extra),
            ExprKind::Loop(expr) => self.visit_loop_expr(expr, new_extra),
            ExprKind::Match(expr) => self.visit_match_expr(expr, new_extra),
            ExprKind::Block(expr) => self.visit_block_expr(expr, new_extra),
            ExprKind::Assign(expr) => self.visit_assign_expr(expr, new_extra),
            ExprKind::AssignOp(expr) => self.visit_assign_op_expr(expr, new_extra),
            ExprKind::Field(expr) => self.visit_field_expr(expr, new_extra),
            ExprKind::Index(expr) => self.visit_index_expr(expr, new_extra),
            ExprKind::Range(expr) => self.visit_range_expr(expr, new_extra),
            ExprKind::Underscore(expr) => self.visit_underscore_expr(expr, new_extra),
            ExprKind::Path(expr) => self.visit_path_expr(expr, new_extra),
            ExprKind::AddrOf(expr) => self.visit_addr_of_expr(expr, new_extra),
            ExprKind::Break(expr) => self.visit_break_expr(expr, new_extra),
            ExprKind::Continue(expr) => self.visit_continue_expr(expr, new_extra),
            ExprKind::Ret(expr) => self.visit_ret_expr(expr, new_extra),
            ExprKind::Struct(expr) => self.visit_struct_expr(expr, new_extra),
            ExprKind::Repeat(expr) => self.visit_repeat_expr(expr, new_extra),
        }
    }

    fn visit_array_expr<'tmp>(
        &mut self,
        ArrayExpr(exprs): &'ast ArrayExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        let intern = self.analyzer.get_expr_type(&extra.self_id);
        let ty = self.transform_interned_ty_faithfully(intern);

        let inner_ty = ty.as_array().unwrap().0.clone();
        let value = self.build_core_alloca(ty.clone(), None);

        for (i, expr) in exprs.iter().enumerate() {
            let v = self.visit_expr(expr, extra)?;
            let index = ValueId::Const(self.core_module.borrow_mut().add_i32_const(i as u32));
            let ith =
                self.core_builder
                    .build_getelementptr(inner_ty.clone(), value, vec![index], None);
            self.store_to_ptr(ValueId::Inst(ith), v);
        }

        self.set_core_expr_value(
            extra.self_id,
            CoreValueContainer {
                value,
                kind: CoreContainerKind::Ptr(ty.clone()),
            },
        );

        Some(ValuePtrContainer {
            value_ptr: value,
            kind: ContainerKind::Ptr(ty),
        })
    }

    fn visit_const_block_expr<'tmp>(
        &mut self,
        _expr: &'ast ConstBlockExpr,
        _extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        impossible!()
    }

    fn visit_call_expr<'tmp>(
        &mut self,
        CallExpr(fn_expr, args_expr): &'ast CallExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        let result = self.analyzer.get_expr_result(&fn_expr.id);
        if result.value_index
            == ValueIndex::Place(PlaceValueIndex {
                name: "exit".into(),
                kind: crate::semantics::value::ValueIndexKind::Global { scope_id: 0 },
            })
        {
            self.visit_ret_expr_impl(Some(args_expr.first().unwrap()), extra);
            return None;
        };

        let fn_value = self.visit_expr(fn_expr, extra)?;
        let func = self
            .core_module
            .borrow()
            .as_function_value(fn_value.value_ptr)
            .expect("expected call target to be a function value");
        let args = args_expr
            .iter()
            .map(|x| self.visit_expr(x, extra))
            .collect::<Option<Vec<_>>>()?;
        let calling_args: Vec<_> = args
            .into_iter()
            .flat_map(|x| self.get_value_presentation(x).flatten())
            .collect();
        let sret = self.core_module.borrow().sret_type(func);

        if let Some(ty) = sret {
            let ptr = self.build_core_alloca(ty.clone(), None);
            self.core_builder
                .build_call(func, once(ptr).chain(calling_args).collect(), None);
            let value = CoreValueContainer {
                value: ptr,
                kind: CoreContainerKind::Ptr(ty),
            };
            self.set_core_expr_value(extra.self_id, value.clone());
            Some(value.into())
        } else {
            let ins = self.core_builder.build_call(func, calling_args, None);
            let value = CoreValueContainer {
                value: ValueId::Inst(ins),
                kind: CoreContainerKind::Raw { fat: None },
            };
            self.set_core_expr_value(extra.self_id, value.clone());
            Some(value.into())
        }
    }

    fn visit_method_call_expr<'tmp>(
        &mut self,
        MethodCallExpr { receiver, args, .. }: &'ast MethodCallExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        let mut receiver_value = self.visit_expr(receiver, extra)?;
        let arg_values = args
            .iter()
            .map(|x| self.visit_expr(x, extra))
            .collect::<Option<Vec<_>>>()?;

        let analyzer_value = self.analyzer.get_expr_value(&extra.self_id);
        let (level, derefed_ty, index, self_by_ref) = analyzer_value.kind.as_method_call().unwrap();
        let mut remove_self_index = index.clone();
        if let PlaceValueIndex {
            name: _,
            kind: ValueIndexKind::Impl { ty, for_trait: _ },
        } = &mut remove_self_index
        {
            *ty = ty.remove_implicit_self(extra.self_ty);
        }

        let mut self_by_ref = *self_by_ref;
        let receiver_ty = if self_by_ref {
            self.transform_ty_faithfully(&ResolvedTy::ref_type(
                *derefed_ty,
                crate::semantics::resolved_ty::RefMutability::Mut,
            ))
        } else {
            self.transform_interned_ty_faithfully(*derefed_ty)
        };
        if level.is_not() && self_by_ref {
            receiver_value = ValuePtrContainer {
                value_ptr: self.get_value_ptr(receiver_value).value_ptr,
                kind: ContainerKind::Raw { fat: None },
            };
            self_by_ref = false;
        }

        let derefed_value = self.deref_impl(receiver_value, level, &receiver_ty, self_by_ref);

        let query_result = self.get_value_by_index(&ValueIndex::Place(remove_self_index.clone()));
        let core_query_result =
            self.try_get_core_value_by_index(&ValueIndex::Place(remove_self_index));

        let ValueKind::Normal(fn_value) = query_result else {
            if let Some(core_query_result) = core_query_result {
                let core_value = self.special_method_call_core(core_query_result);
                self.set_core_expr_value(extra.self_id, core_value);
            }
            return Some(self.special_method_call(query_result));
        };
        let func = self
            .core_module
            .borrow()
            .as_function_value(fn_value.value_ptr)
            .expect("expected method target to be a function value");

        let calling_args: Vec<_> = once(derefed_value)
            .chain(arg_values)
            .flat_map(|x| self.get_value_presentation(x).flatten())
            .collect();
        let sret = self.core_module.borrow().sret_type(func);

        if let Some(ty) = sret {
            let ptr = self.build_core_alloca(ty.clone(), None);
            self.core_builder
                .build_call(func, once(ptr).chain(calling_args).collect(), None);
            let value = CoreValueContainer {
                value: ptr,
                kind: CoreContainerKind::Ptr(ty),
            };
            self.set_core_expr_value(extra.self_id, value.clone());
            Some(value.into())
        } else {
            let ins = self.core_builder.build_call(func, calling_args, None);
            let value = CoreValueContainer {
                value: ValueId::Inst(ins),
                kind: CoreContainerKind::Raw { fat: None },
            };
            self.set_core_expr_value(extra.self_id, value.clone());
            Some(value.into())
        }
    }

    fn visit_tup_expr<'tmp>(
        &mut self,
        TupExpr(exprs, force): &'ast TupExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        match (&exprs[..], force) {
            ([], _) => None,
            ([expr], false) => {
                let value = self.visit_expr(expr, extra);
                if let Some(core_value) = self.core_branch_value(expr) {
                    self.set_core_expr_value(extra.self_id, core_value);
                }
                value
            }
            _ => {
                let intern = self.analyzer.get_expr_type(&extra.self_id);
                let ty = self.transform_interned_ty_faithfully(intern);
                let p = self.build_core_alloca(ty.clone(), None);
                for (i, expr) in exprs.iter().enumerate() {
                    let expr_value = self.visit_expr(expr, extra)?;
                    let zero = ValueId::Const(self.core_module.borrow_mut().add_i32_const(0));
                    let index =
                        ValueId::Const(self.core_module.borrow_mut().add_i32_const(i as u32));
                    let gep = self.core_builder.build_getelementptr(
                        ty.clone(),
                        p,
                        vec![zero, index],
                        None,
                    );
                    self.store_to_ptr(ValueId::Inst(gep), expr_value);
                }
                self.set_core_expr_value(
                    extra.self_id,
                    CoreValueContainer {
                        value: p,
                        kind: CoreContainerKind::Ptr(ty.clone()),
                    },
                );

                Some(ValuePtrContainer {
                    value_ptr: p,
                    kind: ContainerKind::Ptr(ty),
                })
            }
        }
    }

    fn visit_binary_expr<'tmp>(
        &mut self,
        BinaryExpr(bin_op, expr1, expr2): &'ast BinaryExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        match bin_op {
            BinOp::Add
            | BinOp::Sub
            | BinOp::Mul
            | BinOp::Div
            | BinOp::Rem
            | BinOp::BitXor
            | BinOp::BitAnd
            | BinOp::BitOr
            | BinOp::Shl
            | BinOp::Shr => self.visit_binary(*bin_op, expr1, expr2, extra),

            BinOp::And | BinOp::Or => self.visit_logic(*bin_op, expr1, expr2, extra),

            BinOp::Eq | BinOp::Lt | BinOp::Le | BinOp::Ne | BinOp::Ge | BinOp::Gt => {
                self.visit_compare(*bin_op, expr1, expr2, extra)
            }
        }
    }

    fn visit_unary_expr<'tmp>(
        &mut self,
        UnaryExpr(un_op, expr): &'ast UnaryExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        let expr_value = self.visit_expr(expr, extra)?;
        let raw = self.get_raw_value(expr_value);

        match un_op {
            UnOp::Deref => {
                // 此时 expr_value 的 raw 必定为指针类型
                let ty = self
                    .transform_interned_ty_faithfully(self.analyzer.get_expr_type(&extra.self_id));
                let value = ValuePtrContainer {
                    value_ptr: raw,
                    kind: ContainerKind::Ptr(ty),
                };
                let ty = self
                    .transform_interned_ty_faithfully(self.analyzer.get_expr_type(&extra.self_id));
                self.set_core_expr_value(
                    extra.self_id,
                    CoreValueContainer {
                        value: raw,
                        kind: CoreContainerKind::Ptr(ty),
                    },
                );
                Some(value)
            }
            UnOp::Not => {
                let value = self.core_builder.build_bitwise_not(raw);
                self.set_core_expr_value(
                    extra.self_id,
                    CoreValueContainer {
                        value: ValueId::Inst(value),
                        kind: CoreContainerKind::Raw { fat: None },
                    },
                );
                Some(ValuePtrContainer {
                    value_ptr: ValueId::Inst(value),
                    kind: ContainerKind::Raw { fat: None },
                })
            }
            UnOp::Neg => {
                let value = self.core_builder.build_neg(raw);
                self.set_core_expr_value(
                    extra.self_id,
                    CoreValueContainer {
                        value: ValueId::Inst(value),
                        kind: CoreContainerKind::Raw { fat: None },
                    },
                );
                Some(ValuePtrContainer {
                    value_ptr: ValueId::Inst(value),
                    kind: ContainerKind::Raw { fat: None },
                })
            }
        }
    }

    fn visit_lit_expr<'tmp>(
        &mut self,
        LitExpr {
            kind,
            symbol: _,
            suffix: _,
        }: &'ast LitExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        let expr_result = self.analyzer.get_expr_result(&extra.self_id);
        let expr_value = self.analyzer.get_value_by_index(&expr_result.value_index);
        let constant = expr_value.kind.as_constant().unwrap();

        let value = match kind {
            LitKind::Bool => CoreValueContainer {
                value: ValueId::Const(
                    self.core_module
                        .borrow_mut()
                        .add_i1_const(*constant.as_constant_int().unwrap() != 0),
                ),
                kind: CoreContainerKind::Raw { fat: None },
            },
            LitKind::Char => CoreValueContainer {
                value: ValueId::Const(
                    self.core_module
                        .borrow_mut()
                        .add_int_const(8, *constant.as_constant_int().unwrap() as i64),
                ),
                kind: CoreContainerKind::Raw { fat: None },
            },
            LitKind::Integer => CoreValueContainer {
                value: ValueId::Const(
                    self.core_module
                        .borrow_mut()
                        .add_i32_const(*constant.as_constant_int().unwrap()),
                ),
                kind: CoreContainerKind::Raw { fat: None },
            },
            LitKind::Str | LitKind::StrRaw(_) => {
                let string = constant.as_constant_string().unwrap();
                let constant = self
                    .core_module
                    .borrow_mut()
                    .add_string_const(string.clone());
                let ty = self.core_module.borrow().const_data(constant).ty.clone();
                let global = self.core_module.borrow_mut().add_global(
                    format!(".{}.str", extra.self_id),
                    ty,
                    crate::ir::core_value::GlobalKind::GlobalVariable {
                        is_constant: true,
                        initializer: Some(constant),
                    },
                );
                CoreValueContainer {
                    value: ValueId::Global(global),
                    kind: CoreContainerKind::Raw {
                        fat: Some(ValueId::Const(
                            self.core_module
                                .borrow_mut()
                                .add_i32_const(string.len() as u32),
                        )),
                    },
                }
            }
            _ => impossible!(),
        };
        self.set_core_expr_value(extra.self_id, value.clone());
        Some(value.into())
    }

    fn visit_cast_expr<'tmp>(
        &mut self,
        CastExpr(expr, _): &'ast CastExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        let expr_value = self.visit_expr(expr, extra)?;
        let expr_raw = self.get_raw_value(expr_value);
        let ty = self.transform_interned_ty_faithfully(self.analyzer.get_expr_type(&extra.self_id));

        let src_bits = self
            .core_module
            .borrow()
            .value_ty(expr_raw)
            .as_int()
            .unwrap()
            .0;
        let target_bits = ty.as_int().unwrap().0;
        let value = if src_bits < target_bits {
            ValueId::Inst(self.core_builder.build_zext(expr_raw, ty.clone(), None))
        } else if src_bits == target_bits {
            expr_raw
        } else {
            impossible!()
        };
        self.set_core_expr_value(
            extra.self_id,
            CoreValueContainer {
                value,
                kind: CoreContainerKind::Raw { fat: None },
            },
        );

        Some(ValuePtrContainer {
            value_ptr: value,
            kind: ContainerKind::Raw { fat: None },
        })
    }

    fn visit_let_expr<'tmp>(
        &mut self,
        _expr: &'ast LetExpr,
        _extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        impossible!()
    }

    fn visit_if_expr<'tmp>(
        &mut self,
        IfExpr(cond, body, else_expr): &'ast IfExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        let cond_value = self.visit_expr(cond, extra)?;
        let cond_raw = self.get_raw_value(cond_value);
        let current_function = self.core_builder.get_current_function();
        let current_bb = self.core_builder.get_current_basic_block();
        let take_bb = self
            .core_builder
            .append_block(current_function, Some(".take"));

        self.core_builder.locate_end(current_function, take_bb);
        let take_value = self
            .visit_block_expr(
                body,
                ExprExtra {
                    self_id: body.id,
                    ..extra
                },
            )
            .map(|x| self.get_value_presentation(x));
        let take_value = self.legacy_non_void_value(take_value);
        let new_take_bb = self.core_builder.get_current_basic_block();

        if let Some(else_expr) = else_expr {
            let else_bb = self
                .core_builder
                .append_block(current_function, Some(".else"));
            let next_bb = self
                .core_builder
                .append_block(current_function, Some(".next"));
            self.try_build_branch(next_bb, &body.id);
            self.core_builder.locate_end(current_function, current_bb);
            self.try_build_conditional_branch(cond_raw, take_bb, else_bb, &cond.id);
            self.core_builder.locate_end(current_function, else_bb);
            let else_value = self
                .visit_expr(else_expr, extra)
                .map(|x| self.get_value_presentation(x));
            let else_value = self.legacy_non_void_value(else_value);
            let new_else_bb = self.core_builder.get_current_basic_block();
            self.try_build_branch(next_bb, &else_expr.id);
            self.core_builder.locate_end(current_function, next_bb);
            if !self
                .analyzer
                .get_expr_result(&extra.self_id)
                .interrupt
                .is_not()
            {
                self.core_builder.build_unreachable();
            }
            match (take_value, else_value) {
                (None, None) => {
                    self.clear_core_expr_value(&extra.self_id);
                    None
                }
                (None, Some(else_value)) => {
                    let ty = self.get_value_type(&else_value);
                    let v = self.core_builder.build_phi(
                        ty,
                        vec![(else_value.value_ptr, new_else_bb)],
                        None,
                    );
                    let value = ValuePtrContainer {
                        value_ptr: ValueId::Inst(v),
                        kind: else_value.kind,
                    };
                    self.set_core_expr_value(
                        extra.self_id,
                        CoreValueContainer {
                            value: value.value_ptr,
                            kind: match value.kind.clone() {
                                ContainerKind::Raw { fat } => CoreContainerKind::Raw { fat },
                                ContainerKind::Ptr(ty) => CoreContainerKind::Ptr(ty),
                            },
                        },
                    );
                    Some(value)
                }
                (Some(take_value), None) => {
                    let ty = self.get_value_type(&take_value);
                    let v = self.core_builder.build_phi(
                        ty,
                        vec![(take_value.value_ptr, new_take_bb)],
                        None,
                    );
                    let value = ValuePtrContainer {
                        value_ptr: ValueId::Inst(v),
                        kind: take_value.kind,
                    };
                    self.set_core_expr_value(
                        extra.self_id,
                        CoreValueContainer {
                            value: value.value_ptr,
                            kind: match value.kind.clone() {
                                ContainerKind::Raw { fat } => CoreContainerKind::Raw { fat },
                                ContainerKind::Ptr(ty) => CoreContainerKind::Ptr(ty),
                            },
                        },
                    );
                    Some(value)
                }
                (Some(v1), Some(v2)) => {
                    debug_assert_eq!(self.get_value_type(&v1), self.get_value_type(&v2));
                    let ty = self.get_value_type(&v1);
                    let v = self.core_builder.build_phi(
                        ty,
                        vec![(v1.value_ptr, new_take_bb), (v2.value_ptr, new_else_bb)],
                        None,
                    );
                    let value = ValuePtrContainer {
                        value_ptr: ValueId::Inst(v),
                        kind: v1.kind,
                    };
                    self.set_core_expr_value(
                        extra.self_id,
                        CoreValueContainer {
                            value: value.value_ptr,
                            kind: match value.kind.clone() {
                                ContainerKind::Raw { fat } => CoreContainerKind::Raw { fat },
                                ContainerKind::Ptr(ty) => CoreContainerKind::Ptr(ty),
                            },
                        },
                    );
                    Some(value)
                }
            }
        } else {
            let next_bb = self
                .core_builder
                .append_block(current_function, Some(".next"));
            self.try_build_branch(next_bb, &body.id);
            self.core_builder.locate_end(current_function, current_bb);
            self.try_build_conditional_branch(cond_raw, take_bb, next_bb, &cond.id);
            self.core_builder.locate_end(current_function, next_bb);
            self.clear_core_expr_value(&extra.self_id);
            None
        }
    }

    fn visit_while_expr<'tmp>(
        &mut self,
        WhileExpr(cond, body): &'ast WhileExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        let current_function = self.core_builder.get_current_function();
        let cond_bb = self
            .core_builder
            .append_block(current_function, Some(".cond"));
        let body_bb = self
            .core_builder
            .append_block(current_function, Some(".body"));
        let next_bb = self
            .core_builder
            .append_block(current_function, Some(".next"));

        self.core_builder.build_branch(cond_bb);
        self.core_builder.locate_end(current_function, cond_bb);
        let cond_value = self.visit_expr(cond, extra)?;
        let cond_raw = self.get_raw_value(cond_value);
        self.try_build_conditional_branch(cond_raw, body_bb, next_bb, &cond.id);
        self.core_builder.locate_end(current_function, body_bb);
        let body_value = self.visit_block_expr(
            body,
            ExprExtra {
                self_id: body.id,
                core_cycle_info: Some(CoreCycleInfo {
                    continue_bb: cond_bb,
                    next_bb,
                    value: None,
                }),
                ..extra
            },
        );
        debug_assert!(body_value.is_none());
        self.try_build_branch(cond_bb, &body.id);

        self.core_builder.locate_end(current_function, next_bb);
        self.clear_core_expr_value(&extra.self_id);

        None
    }

    fn visit_for_loop_expr<'tmp>(
        &mut self,
        _expr: &'ast ForLoopExpr,
        _extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        impossible!()
    }

    fn visit_loop_expr<'tmp>(
        &mut self,
        LoopExpr(body): &'ast LoopExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        let current_function = self.core_builder.get_current_function();
        let loop_bb = self
            .core_builder
            .append_block(current_function, Some(".loop"));
        let next_bb = self
            .core_builder
            .append_block(current_function, Some(".next"));

        let ty = self.transform_interned_ty_impl(
            self.analyzer.get_expr_type(&extra.self_id),
            crate::irgen::ty::TransformTypeConfig::FirstClassNoUnit,
        );
        let loop_value: Option<ValueId> = if ty.is_void() {
            None
        } else {
            Some(self.build_core_alloca(ty.clone(), None))
        };

        self.core_builder.build_branch(loop_bb);

        self.core_builder.locate_end(current_function, loop_bb);
        self.visit_block_expr(
            body,
            ExprExtra {
                scope_id: extra.scope_id,
                self_id: body.id,
                core_cycle_info: Some(CoreCycleInfo {
                    continue_bb: loop_bb,
                    next_bb,
                    value: loop_value,
                }),
                ..extra
            },
        );
        self.try_build_branch(loop_bb, &body.id);

        self.core_builder.locate_end(current_function, next_bb);

        match loop_value {
            Some(loop_value) => {
                let (value_ptr, kind) = if ty.is_aggregate_type() {
                    (loop_value, ContainerKind::Ptr(ty.clone()))
                } else {
                    (
                        ValueId::Inst(self.core_builder.build_load(ty.clone(), loop_value, None)),
                        ContainerKind::Raw { fat: None },
                    )
                };
                self.set_core_expr_value(
                    extra.self_id,
                    CoreValueContainer {
                        value: value_ptr,
                        kind: match kind.clone() {
                            ContainerKind::Raw { fat } => CoreContainerKind::Raw { fat },
                            ContainerKind::Ptr(ty) => CoreContainerKind::Ptr(ty),
                        },
                    },
                );

                Some(ValuePtrContainer { value_ptr, kind })
            }
            None => {
                if !self
                    .analyzer
                    .get_expr_result(&extra.self_id)
                    .interrupt
                    .is_not()
                {
                    self.core_builder.build_unreachable();
                }
                self.clear_core_expr_value(&extra.self_id);

                None
            }
        }
    }

    fn visit_match_expr<'tmp>(
        &mut self,
        _expr: &'ast MatchExpr,
        _extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        impossible!()
    }

    fn visit_block_expr<'tmp>(
        &mut self,
        BlockExpr { stmts, id, span: _ }: &'ast BlockExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        let mut v = None;
        let mut core_v = None;

        for stmt in stmts {
            let t = self.visit_stmt(
                stmt,
                ExprExtra {
                    scope_id: *id,
                    ..extra
                },
            );
            let result = self.analyzer.get_stmt_result(&stmt.id);
            let interrupt = match result {
                crate::semantics::stmt::StmtResult::Expr(expr_id) => {
                    self.analyzer.get_expr_result(expr_id).interrupt
                }
                crate::semantics::stmt::StmtResult::Else { interrupt } => *interrupt,
            };
            if interrupt.is_not() {
                if let Some(i) = t {
                    let _ = v.insert(i);
                    if let StmtKind::Expr(expr) = &stmt.kind
                        && let Some(core_value) = self.core_branch_value(expr)
                    {
                        let _ = core_v.insert(core_value);
                    }
                }
            } else {
                return None;
            }
        }

        if let Some(core_value) = core_v {
            self.set_core_expr_value(*id, core_value);
        } else {
            self.clear_core_expr_value(id);
        }
        v
    }

    fn visit_assign_expr<'tmp>(
        &mut self,
        AssignExpr(left, right): &'ast AssignExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        let right_value = self.visit_expr(right, extra)?;
        let core_right_value = self.core_branch_value(right);
        self.destructing_assign(left, extra, right_value)?;
        if let Some(core_right_value) = core_right_value {
            self.core_destructing_assign(left, extra, core_right_value)?;
        }
        None
    }

    fn visit_assign_op_expr<'tmp>(
        &mut self,
        AssignOpExpr(op, left, right): &'ast AssignOpExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        let left_value = self.visit_expr(left, extra)?;
        let right_value = self.visit_expr(right, extra)?;
        let left_raw = self.get_raw_value(left_value.clone());
        let right_raw = self.get_raw_value(right_value);
        let intern = self.analyzer.get_expr_type(&left.id);

        let bin_op = match op {
            AssignOp::AddAssign => BinOp::Add,
            AssignOp::SubAssign => BinOp::Sub,
            AssignOp::MulAssign => BinOp::Mul,
            AssignOp::DivAssign => BinOp::Div,
            AssignOp::RemAssign => BinOp::Rem,
            AssignOp::BitXorAssign => BinOp::BitXor,
            AssignOp::BitAndAssign => BinOp::BitAnd,
            AssignOp::BitOrAssign => BinOp::BitOr,
            AssignOp::ShlAssign => BinOp::Shl,
            AssignOp::ShrAssign => BinOp::Shr,
        };

        let v = self.visit_binary_impl_core(bin_op, left_raw, right_raw, intern);
        let ptr = self.get_value_ptr(left_value);
        self.store_to_ptr(ptr.value_ptr, v.into());

        None
    }

    fn visit_field_expr<'tmp>(
        &mut self,
        FieldExpr(expr, ..): &'ast FieldExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        let expr_value = self.visit_expr(expr, extra)?;

        let analyzer_value = self.analyzer.get_expr_value(&extra.self_id);
        let (deref_level, struct_intern, pos) = analyzer_value.kind.as_extract_element().unwrap();
        let struct_ty = self.transform_interned_ty_faithfully(*struct_intern);
        let derefed_value = self.deref(expr_value, deref_level, &struct_ty);

        let zero = ValueId::Const(self.core_module.borrow_mut().add_i32_const(0));
        let index = ValueId::Const(
            self.core_module
                .borrow_mut()
                .add_i32_const(pos.unwrap() as u32),
        );
        let v = self.core_builder.build_getelementptr(
            struct_ty.clone(),
            derefed_value.value_ptr,
            vec![zero, index],
            None,
        );

        let ty = self.transform_interned_ty_faithfully(self.analyzer.get_expr_type(&extra.self_id));
        self.set_core_expr_value(
            extra.self_id,
            CoreValueContainer {
                value: ValueId::Inst(v),
                kind: CoreContainerKind::Ptr(ty.clone()),
            },
        );

        Some(ValuePtrContainer {
            value_ptr: ValueId::Inst(v),
            kind: ContainerKind::Ptr(ty),
        })
    }

    fn visit_index_expr<'tmp>(
        &mut self,
        IndexExpr(array_expr, index_expr): &'ast IndexExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        let array_value = self.visit_expr(array_expr, extra)?;
        let index_value = self.visit_expr(index_expr, extra)?;
        let index_raw = self.get_raw_value(index_value);

        let analyzer_value = self.analyzer.get_expr_value(&extra.self_id);
        let (deref_level, intern, _) = analyzer_value.kind.as_extract_element().unwrap();
        let array_ty = self.transform_interned_ty_faithfully(*intern);
        let inner_ty = array_ty.as_array().unwrap().0.clone();
        let derefed_value = self.deref(array_value, deref_level, &array_ty);

        let zero = ValueId::Const(self.core_module.borrow_mut().add_i32_const(0));
        let v = self.core_builder.build_getelementptr(
            array_ty.clone(),
            derefed_value.value_ptr,
            vec![zero, index_raw],
            None,
        );
        self.set_core_expr_value(
            extra.self_id,
            CoreValueContainer {
                value: ValueId::Inst(v),
                kind: CoreContainerKind::Ptr(inner_ty.clone()),
            },
        );

        Some(ValuePtrContainer {
            value_ptr: ValueId::Inst(v),
            kind: ContainerKind::Ptr(inner_ty),
        })
    }

    fn visit_range_expr<'tmp>(
        &mut self,
        _expr: &'ast RangeExpr,
        _extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        impossible!()
    }

    fn visit_underscore_expr<'tmp>(
        &mut self,
        _expr: &'ast UnderscoreExpr,
        _extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        impossible!()
    }

    fn visit_path_expr<'tmp>(
        &mut self,
        _expr: &'ast PathExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        let result = self.analyzer.get_expr_result(&extra.self_id);
        let index = &result.value_index;
        let value = self.get_value_by_index(index).into_normal().unwrap();
        if let Some(core_kind) = self.try_get_core_value_by_index(index)
            && let Ok(core_value) = core_kind.into_normal()
        {
            self.set_core_expr_value(extra.self_id, core_value);
        }
        Some(value)
    }

    fn visit_addr_of_expr<'tmp>(
        &mut self,
        AddrOfExpr(_, inner_expr): &'ast AddrOfExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        let v = self.visit_expr(inner_expr, extra)?;
        if let Some(core_value) = self.core_branch_value(inner_expr) {
            let core_ptr = self.core_get_value_ptr(core_value);
            self.set_core_expr_value(
                extra.self_id,
                CoreValueContainer {
                    value: core_ptr.value,
                    kind: CoreContainerKind::Raw { fat: None },
                },
            );
        }
        Some(ValuePtrContainer {
            value_ptr: self.get_value_ptr(v).value_ptr,
            kind: ContainerKind::Raw { fat: None },
        })
    }

    fn visit_break_expr<'tmp>(
        &mut self,
        BreakExpr(inner_expr): &'ast BreakExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        let v = if let Some(e) = inner_expr {
            self.visit_expr(e, extra)
        } else {
            None
        };
        let core_cycle_info = extra.core_cycle_info.unwrap();
        if let Some(v) = v
            && let Some(dest) = core_cycle_info.value
        {
            self.store_to_ptr(dest, v);
        }

        if let Some(e) = inner_expr {
            self.try_build_branch(core_cycle_info.next_bb, &e.id);
        } else {
            self.core_builder.build_branch(core_cycle_info.next_bb);
        }

        None
    }

    fn visit_continue_expr<'tmp>(
        &mut self,
        ContinueExpr: &'ast ContinueExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        let core_cycle_info = extra.core_cycle_info.unwrap();
        self.core_builder.build_branch(core_cycle_info.continue_bb);

        None
    }

    fn visit_ret_expr<'tmp>(
        &mut self,
        RetExpr(inner_expr): &'ast RetExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        self.visit_ret_expr_impl(inner_expr.as_ref().map(|x| x.as_ref()), extra);
        None
    }

    fn visit_struct_expr<'tmp>(
        &mut self,
        StructExpr { fields, .. }: &'ast StructExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        let intern = self.analyzer.get_expr_type(&extra.self_id);
        let ty = self.transform_interned_ty_faithfully(intern);

        let value = self.build_core_alloca(ty.clone(), None);
        let indexes = self
            .analyzer
            .get_expr_value(&extra.self_id)
            .kind
            .as_struct()
            .unwrap();

        for (ExprField { expr, .. }, index) in zip(fields, indexes) {
            let v = self.visit_expr(expr, extra)?;
            let zero = ValueId::Const(self.core_module.borrow_mut().add_i32_const(0));
            let index = ValueId::Const(self.core_module.borrow_mut().add_i32_const(*index as u32));
            let ith =
                self.core_builder
                    .build_getelementptr(ty.clone(), value, vec![zero, index], None);
            self.store_to_ptr(ValueId::Inst(ith), v);
        }
        self.set_core_expr_value(
            extra.self_id,
            CoreValueContainer {
                value,
                kind: CoreContainerKind::Ptr(ty.clone()),
            },
        );

        Some(ValuePtrContainer {
            value_ptr: value,
            kind: ContainerKind::Ptr(ty),
        })
    }

    fn visit_repeat_expr<'tmp>(
        &mut self,
        RepeatExpr(inner_expr, _): &'ast RepeatExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        let inner_value = self.visit_expr(inner_expr, extra)?;

        let intern = self.analyzer.get_expr_type(&extra.self_id);
        let ty = self.transform_interned_ty_faithfully(intern);
        let ArrayType(inner_ty, repeat_num) = ty.as_array().unwrap().clone();
        let value = self.build_core_alloca(ty.clone(), None);

        let current_function = self.core_builder.get_current_function();
        let repeat_loop_header_bb = self
            .core_builder
            .append_block(current_function, Some(".repeat_loop_header"));
        let repeat_loop_body_bb = self
            .core_builder
            .append_block(current_function, Some(".repeat_loop_body"));
        let repeat_loop_next_bb = self
            .core_builder
            .append_block(current_function, Some(".repeat_loop_next"));
        let repeat_counter =
            self.build_core_alloca(self.context.i32_type().into(), Some(".repeat_counter"));
        let zero = ValueId::Const(self.core_module.borrow_mut().add_i32_const(0));
        self.core_builder.build_store(zero, repeat_counter);
        self.core_builder.build_branch(repeat_loop_header_bb);

        self.core_builder
            .locate_end(current_function, repeat_loop_header_bb);
        let repeat_counter_v =
            self.core_builder
                .build_load(self.context.i32_type().into(), repeat_counter, None);
        let repeat_num = ValueId::Const(self.core_module.borrow_mut().add_i32_const(repeat_num));
        let cond = self.core_builder.build_icmp(
            crate::ir::core_inst::ICmpCode::Eq,
            ValueId::Inst(repeat_counter_v),
            repeat_num,
            None,
        );
        self.core_builder.build_conditional_branch(
            ValueId::Inst(cond),
            repeat_loop_next_bb,
            repeat_loop_body_bb,
        );

        self.core_builder
            .locate_end(current_function, repeat_loop_body_bb);
        let ith_ptr = self.core_builder.build_getelementptr(
            inner_ty,
            value,
            vec![ValueId::Inst(repeat_counter_v)],
            None,
        );
        self.store_to_ptr(ValueId::Inst(ith_ptr), inner_value.clone());
        let one = ValueId::Const(self.core_module.borrow_mut().add_i32_const(1));
        let new_counter_v = self.core_builder.build_binary(
            crate::ir::core_inst::BinaryOpcode::Add,
            self.context.i32_type().into(),
            ValueId::Inst(repeat_counter_v),
            one,
            None,
        );
        self.core_builder
            .build_store(ValueId::Inst(new_counter_v), repeat_counter);
        self.core_builder.build_branch(repeat_loop_header_bb);

        self.core_builder
            .locate_end(current_function, repeat_loop_next_bb);
        self.set_core_expr_value(
            extra.self_id,
            CoreValueContainer {
                value,
                kind: CoreContainerKind::Ptr(ty.clone()),
            },
        );

        Some(ValuePtrContainer {
            value_ptr: value,
            kind: ContainerKind::Ptr(ty),
        })
    }

    fn visit_pat<'tmp>(
        &mut self,
        Pat { kind, id, span: _ }: &'ast Pat,
        extra: Self::PatExtra<'tmp>,
    ) -> Self::PatRes<'_> {
        let new_extra = PatExtra {
            self_id: *id,
            ..extra
        };
        match kind {
            PatKind::Wild(pat) => self.visit_wild_pat(pat, new_extra),
            PatKind::Ident(pat) => self.visit_ident_pat(pat, new_extra),
            PatKind::Struct(pat) => self.visit_struct_pat(pat, new_extra),
            PatKind::Or(pat) => self.visit_or_pat(pat, new_extra),
            PatKind::Path(pat) => self.visit_path_pat(pat, new_extra),
            PatKind::Tuple(pat) => self.visit_tuple_pat(pat, new_extra),
            PatKind::Ref(pat) => self.visit_ref_pat(pat, new_extra),
            PatKind::Lit(pat) => self.visit_lit_pat(pat, new_extra),
            PatKind::Range(pat) => self.visit_range_pat(pat, new_extra),
            PatKind::Slice(pat) => self.visit_slice_pat(pat, new_extra),
            PatKind::Rest(pat) => self.visit_rest_pat(pat, new_extra),
        }
    }

    fn visit_wild_pat<'tmp>(
        &mut self,
        _pat: &'ast WildPat,
        _extra: Self::PatExtra<'tmp>,
    ) -> Self::PatRes<'_> {
        impossible!()
    }

    fn visit_ident_pat<'tmp>(
        &mut self,
        IdentPat(mode, ident, _restriction): &'ast IdentPat,
        extra: Self::PatExtra<'tmp>,
    ) -> Self::PatRes<'_> {
        self.visit_ident_pat_impl(mode, ident, extra)
    }

    fn visit_struct_pat<'tmp>(
        &mut self,
        _pat: &'ast StructPat,
        _extra: Self::PatExtra<'tmp>,
    ) -> Self::PatRes<'_> {
        impossible!()
    }

    fn visit_or_pat<'tmp>(
        &mut self,
        _pat: &'ast OrPat,
        _extra: Self::PatExtra<'tmp>,
    ) -> Self::PatRes<'_> {
        impossible!()
    }

    fn visit_path_pat<'tmp>(
        &mut self,
        PathPat(_, path): &'ast PathPat,
        extra: Self::PatExtra<'tmp>,
    ) -> Self::PatRes<'_> {
        let ident = path.get_ident();
        let mode = BindingMode(crate::ast::ByRef::No, Mutability::Not);
        self.visit_ident_pat_impl(&mode, ident, extra)
    }

    fn visit_tuple_pat<'tmp>(
        &mut self,
        _pat: &'ast TuplePat,
        _extra: Self::PatExtra<'tmp>,
    ) -> Self::PatRes<'_> {
        impossible!()
    }

    fn visit_ref_pat<'tmp>(
        &mut self,
        RefPat(pat, _): &'ast RefPat,
        PatExtra {
            value,
            core_value,
            self_id: _,
            is_temp_value,
        }: Self::PatExtra<'tmp>,
    ) -> Self::PatRes<'_> {
        let new_value = self.get_value_ptr(value);
        let new_core_value = core_value.map(|value| self.core_get_value_ptr(value));
        self.visit_pat(
            pat,
            PatExtra {
                value: new_value,
                core_value: new_core_value,
                self_id: 0,
                is_temp_value,
            },
        )
    }

    fn visit_lit_pat<'tmp>(
        &mut self,
        _pat: &'ast LitPat,
        _extra: Self::PatExtra<'tmp>,
    ) -> Self::PatRes<'_> {
        impossible!()
    }

    fn visit_range_pat<'tmp>(
        &mut self,
        _pat: &'ast RangePat,
        _extra: Self::PatExtra<'tmp>,
    ) -> Self::PatRes<'_> {
        impossible!()
    }

    fn visit_slice_pat<'tmp>(
        &mut self,
        _pat: &'ast SlicePat,
        _extra: Self::PatExtra<'tmp>,
    ) -> Self::PatRes<'_> {
        impossible!()
    }

    fn visit_rest_pat<'tmp>(
        &mut self,
        _pat: &'ast RestPat,
        _extra: Self::PatExtra<'tmp>,
    ) -> Self::PatRes<'_> {
        impossible!()
    }
}
