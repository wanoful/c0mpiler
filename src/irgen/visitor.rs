use std::iter::{once, zip};

use crate::{
    ast::{BindingMode, Crate, Mutability, expr::*, item::*, pat::*, stmt::*},
    impossible,
    ir::{
        attribute::AttributeDiscriminants,
        globalxxx::FunctionPtr,
        ir_type::ArrayType,
        ir_value::{BasicBlockPtr, GlobalObjectPtr, ValuePtr},
    },
    irgen::{
        IRGenerator,
        extra::{CycleInfo, ExprExtra, ItemExtra, PatExtra},
        ty::TransformTypeConfig,
        value::{ContainerKind, ValueKind, ValuePtrContainer},
    },
    semantics::{
        item::AssociatedInfo,
        resolved_ty::ResolvedTy,
        value::{FnAstRefInfo, PlaceValueIndex, ValueIndex, ValueIndexKind},
        visitor::Visitor,
    },
};

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
        let fn_ptr = self.module.get_function(&name_string).expect(&name_string);
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

        let is_aggregate = fn_ptr
            .as_function()
            .get_param_attr(0, AttributeDiscriminants::StructReturn);
        let args = fn_ptr.as_function().args();
        let (ret_ptr, args) = if is_aggregate.is_some() {
            let (ret_ptr, args) = args.split_first().unwrap();
            (Some(ret_ptr), args)
        } else {
            (None, args)
        };

        let bb = self.context.append_basic_block(&fn_ptr, "entry");
        let loc = self.builder.get_location();
        let alloca_loc = self.alloca_builder.get_location();
        self.builder.locate_end(fn_ptr.clone(), bb.clone());
        self.alloca_builder.locate_front(fn_ptr.clone(), bb);

        let mut args_iter = args.iter();
        for (arg_type, param) in zip(arg_types, &decl.inputs) {
            let arg = args_iter.next().unwrap();
            let kind = if arg_type.is_aggregate_type() {
                ContainerKind::Ptr(arg_type)
            } else {
                ContainerKind::Raw {
                    fat: if arg_type.is_fat_ptr() {
                        Some(args_iter.next().unwrap().clone().into())
                    } else {
                        None
                    },
                }
            };
            self.visit_pat(
                &param.pat,
                PatExtra {
                    value: ValuePtrContainer {
                        value_ptr: arg.clone().into(),
                        kind,
                    },
                    core_value: None,
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
                cycle_info: None,
                ret_ptr,
                self_ty: self_ty.as_ref(),
            },
        );

        if let Some(value) = value {
            if let Some(ret_ptr) = ret_ptr {
                self.store_to_ptr(ret_ptr.clone().into(), value);
                self.builder.build_return(None);
            } else {
                self.builder.build_return(Some(self.get_raw_value(value)));
            }
        } else {
            self.try_build_return(None, &body.as_ref().unwrap().id);
        }

        self.builder.set_location(loc);
        self.alloca_builder.set_location(alloca_loc);
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

        self.visit_pat(
            pat,
            PatExtra {
                value,
                core_value: None,
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
        let value = self.build_alloca(ty.clone(), None);

        for (i, expr) in exprs.iter().enumerate() {
            let v = self.visit_expr(expr, extra)?;
            let ith = self.builder.build_getelementptr(
                inner_ty.clone(),
                value.clone().into(),
                vec![self.context.get_i32(i as u32).into()],
                None,
            );
            self.store_to_ptr(ith.into(), v);
        }

        Some(ValuePtrContainer {
            value_ptr: value.into(),
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
        debug_assert!(
            fn_value
                .value_ptr
                .kind
                .as_global_object()
                .is_some_and(|x| x.kind.is_function()),
            "{:?}",
            fn_value
        );
        let args = args_expr
            .iter()
            .map(|x| self.visit_expr(x, extra))
            .collect::<Option<Vec<_>>>()?;

        let func_ptr = FunctionPtr(GlobalObjectPtr(fn_value.value_ptr));
        let is_aggregate = func_ptr
            .as_function()
            .get_param_attr(0, AttributeDiscriminants::StructReturn);

        let calling_args = args
            .into_iter()
            .flat_map(|x| self.get_value_presentation(x).flatten());

        if let Some(attr) = is_aggregate {
            let ty = attr.into_struct_return().unwrap();
            let ptr = self.build_alloca(ty.clone(), None);
            self.builder.build_call(
                func_ptr,
                once(ptr.clone().into()).chain(calling_args).collect(),
                None,
            );
            Some(ValuePtrContainer {
                value_ptr: ptr.into(),
                kind: ContainerKind::Ptr(ty.clone()),
            })
        } else {
            let ins = self
                .builder
                .build_call(func_ptr, calling_args.collect(), None);

            Some(ValuePtrContainer {
                value_ptr: ins.into(),
                kind: ContainerKind::Raw { fat: None },
            })
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

        let query_result = self.get_value_by_index(&ValueIndex::Place(remove_self_index));

        let ValueKind::Normal(fn_value) = query_result else {
            return Some(self.special_method_call(query_result));
        };
        debug_assert!(
            fn_value
                .value_ptr
                .kind
                .as_global_object()
                .is_some_and(|x| x.kind.is_function()),
            "{:?}",
            fn_value
        );
        let func_ptr = FunctionPtr(GlobalObjectPtr(fn_value.value_ptr));
        let is_aggregate = func_ptr
            .as_function()
            .get_param_attr(0, AttributeDiscriminants::StructReturn);

        let calling_args = once(derefed_value)
            .chain(arg_values)
            .flat_map(|x| self.get_value_presentation(x).flatten());

        if let Some(attr) = is_aggregate {
            let ty = attr.into_struct_return().unwrap();
            let ptr = self.build_alloca(ty.clone(), None);
            self.builder.build_call(
                func_ptr,
                once(ptr.clone().into()).chain(calling_args).collect(),
                None,
            );
            Some(ValuePtrContainer {
                value_ptr: ptr.into(),
                kind: ContainerKind::Ptr(ty.clone()),
            })
        } else {
            let ins = self
                .builder
                .build_call(func_ptr, calling_args.collect(), None);

            Some(ValuePtrContainer {
                value_ptr: ins.into(),
                kind: ContainerKind::Raw { fat: None },
            })
        }
    }

    fn visit_tup_expr<'tmp>(
        &mut self,
        TupExpr(exprs, force): &'ast TupExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        match (&exprs[..], force) {
            ([], _) => None,
            ([expr], false) => self.visit_expr(expr, extra),
            _ => {
                let intern = self.analyzer.get_expr_type(&extra.self_id);
                let ty = self.transform_interned_ty_faithfully(intern);
                let p = self.build_alloca(ty.clone(), None);
                for (i, expr) in exprs.iter().enumerate() {
                    let expr_value = self.visit_expr(expr, extra)?;
                    let gep = self.builder.build_getelementptr(
                        ty.clone(),
                        p.clone().into(),
                        vec![
                            self.context.get_i32(0).into(),
                            self.context.get_i32(i as u32).into(),
                        ],
                        None,
                    );
                    self.store_to_ptr(gep.into(), expr_value);
                }

                Some(ValuePtrContainer {
                    value_ptr: p.into(),
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
                Some(ValuePtrContainer {
                    value_ptr: raw,
                    kind: ContainerKind::Ptr(ty),
                })
            }
            UnOp::Not => {
                let value = self.builder.build_bitwise_not(raw);

                Some(ValuePtrContainer {
                    value_ptr: value.into(),
                    kind: ContainerKind::Raw { fat: None },
                })
            }
            UnOp::Neg => {
                let value = self.builder.build_neg(raw);

                Some(ValuePtrContainer {
                    value_ptr: value.into(),
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

        let value: ValuePtr = match kind {
            LitKind::Bool => self
                .context
                .get_i1(*constant.as_constant_int().unwrap() != 0)
                .into(),
            LitKind::Char => self
                .context
                .get_i8(*constant.as_constant_int().unwrap() as u8)
                .into(),
            LitKind::Integer => self
                .context
                .get_i32(*constant.as_constant_int().unwrap())
                .into(),
            LitKind::Str | LitKind::StrRaw(_) => {
                let string = constant.as_constant_string().unwrap();
                let constant = self.context.get_string(string);
                let global = self.module.add_global_variable(
                    true,
                    constant.into(),
                    &format!(".{}.str", extra.self_id),
                );

                global.into()
            }

            _ => impossible!(),
        };

        Some(ValuePtrContainer {
            value_ptr: value,
            kind: ContainerKind::Raw {
                fat: constant
                    .as_constant_string()
                    .map(|x| self.context.get_i32(x.len() as u32).into()),
            },
        })
    }

    fn visit_cast_expr<'tmp>(
        &mut self,
        CastExpr(expr, _): &'ast CastExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        let expr_value = self.visit_expr(expr, extra)?;
        let expr_raw = self.get_raw_value(expr_value);
        let ty = self.transform_interned_ty_faithfully(self.analyzer.get_expr_type(&extra.self_id));

        let src_bits = expr_raw.get_type_as_int().unwrap().0;
        let target_bits = ty.as_int().unwrap().0;
        let value = if src_bits < target_bits {
            self.builder.build_zext(expr_raw, ty, None).into()
        } else if src_bits == target_bits {
            expr_raw
        } else {
            impossible!()
        };

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

        let current_function = self.builder.get_current_function().clone();
        let current_bb = self.builder.get_current_basic_block().clone();

        let take_bb = self.context.append_basic_block(&current_function, ".take");
        let next_bb: BasicBlockPtr;

        self.builder
            .locate_end(current_function.clone(), take_bb.clone());
        let take_value = self
            .visit_block_expr(
                body,
                ExprExtra {
                    self_id: body.id,
                    ..extra
                },
            )
            .map(|x| self.get_value_presentation(x));
        let new_take_bb = self.builder.get_current_basic_block().clone();

        if let Some(else_expr) = else_expr {
            let else_bb = self.context.append_basic_block(&current_function, ".else");
            next_bb = self.context.append_basic_block(&current_function, ".next");
            self.try_build_branch(next_bb.clone(), &body.id);
            self.builder
                .locate_end(current_function.clone(), current_bb);
            self.try_build_conditional_branch(cond_raw, take_bb.clone(), else_bb.clone(), &cond.id);
            self.builder
                .locate_end(current_function.clone(), else_bb.clone());
            let else_value = self
                .visit_expr(else_expr, extra)
                .map(|x| self.get_value_presentation(x));
            let new_else_bb = self.builder.get_current_basic_block().clone();
            self.try_build_branch(next_bb.clone(), &else_expr.id);
            self.builder
                .locate_end(current_function.clone(), next_bb.clone());
            if !self
                .analyzer
                .get_expr_result(&extra.self_id)
                .interrupt
                .is_not()
            {
                self.builder.build_unreachable();
            }
            match (take_value, else_value) {
                (None, None) => None,
                (None, Some(else_value)) => Some(else_value),
                (Some(take_value), None) => Some(take_value),
                (Some(v1), Some(v2)) => {
                    debug_assert_eq!(v1.value_ptr.get_type(), v2.value_ptr.get_type());

                    // phi 指令必须位于基本块的开头
                    let v = self.builder.build_phi(
                        v1.value_ptr.get_type().clone(),
                        vec![
                            (v1.value_ptr, new_take_bb.clone()),
                            (v2.value_ptr, new_else_bb.clone()),
                        ],
                        None,
                    );

                    Some(ValuePtrContainer {
                        value_ptr: v.into(),
                        kind: v1.kind,
                    })
                }
            }
        } else {
            next_bb = self.context.append_basic_block(&current_function, ".next");
            self.try_build_branch(next_bb.clone(), &body.id);
            self.builder
                .locate_end(current_function.clone(), current_bb);
            self.try_build_conditional_branch(cond_raw, take_bb, next_bb.clone(), &cond.id);
            self.builder
                .locate_end(current_function.clone(), next_bb.clone());
            None
        }
    }

    fn visit_while_expr<'tmp>(
        &mut self,
        WhileExpr(cond, body): &'ast WhileExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        let current_function = self.builder.get_current_function().clone();

        let cond_bb = self.context.append_basic_block(&current_function, ".cond");
        let body_bb = self.context.append_basic_block(&current_function, ".body");
        let next_bb = self.context.append_basic_block(&current_function, ".next");

        self.builder.build_branch(cond_bb.clone());
        self.builder
            .locate_end(current_function.clone(), cond_bb.clone());
        let cond_value = self.visit_expr(cond, extra)?;
        self.try_build_conditional_branch(
            self.get_raw_value(cond_value),
            body_bb.clone(),
            next_bb.clone(),
            &cond.id,
        );

        self.builder.locate_end(current_function.clone(), body_bb);
        let body_value = self.visit_block_expr(
            body,
            ExprExtra {
                self_id: body.id,
                cycle_info: Some(CycleInfo {
                    continue_bb: &cond_bb,
                    next_bb: &next_bb,
                    value: None,
                }),
                ..extra
            },
        );
        debug_assert!(body_value.is_none());
        self.try_build_branch(cond_bb.clone(), &body.id);

        self.builder.locate_end(current_function, next_bb.clone());

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
        let current_function = self.builder.get_current_function().clone();

        let loop_bb = self.context.append_basic_block(&current_function, ".loop");
        let next_bb = self.context.append_basic_block(&current_function, ".next");

        let ty = self.transform_interned_ty_impl(
            self.analyzer.get_expr_type(&extra.self_id),
            crate::irgen::ty::TransformTypeConfig::FirstClassNoUnit,
        );
        let loop_value: Option<ValuePtr> = if ty.is_void() {
            None
        } else {
            Some(
                self.build_alloca(self.context.ptr_type().into(), None)
                    .into(),
            )
        };

        self.builder.build_branch(loop_bb.clone());

        self.builder
            .locate_end(current_function.clone(), loop_bb.clone());
        self.visit_block_expr(
            body,
            ExprExtra {
                scope_id: extra.scope_id,
                self_id: body.id,
                cycle_info: Some(CycleInfo {
                    continue_bb: &loop_bb,
                    next_bb: &next_bb,
                    value: loop_value.as_ref(),
                }),
                ..extra
            },
        );
        self.try_build_branch(loop_bb.clone(), &body.id);

        self.builder
            .locate_end(current_function.clone(), next_bb.clone());

        match loop_value {
            Some(loop_value) => {
                let (value_ptr, kind) = if ty.is_aggregate_type() {
                    (
                        self.builder
                            .build_load(self.context.ptr_type().into(), loop_value, None),
                        ContainerKind::Ptr(ty),
                    )
                } else {
                    (
                        self.builder.build_load(ty, loop_value, None),
                        ContainerKind::Raw { fat: None },
                    )
                };

                Some(ValuePtrContainer {
                    value_ptr: value_ptr.into(),
                    kind,
                })
            }
            None => {
                if !self
                    .analyzer
                    .get_expr_result(&extra.self_id)
                    .interrupt
                    .is_not()
                {
                    self.builder.build_unreachable();
                }

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
                }
            } else {
                return None;
            }
        }

        v
    }

    fn visit_assign_expr<'tmp>(
        &mut self,
        AssignExpr(left, right): &'ast AssignExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        let right_value = self.visit_expr(right, extra)?;
        self.destructing_assign(left, extra, right_value)?;
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

        let v = self.visit_binary_impl(bin_op, left_raw, right_raw, intern);
        let ptr = self.get_value_ptr(left_value);
        self.store_to_ptr(ptr.value_ptr, v);

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

        let v = self.builder.build_getelementptr(
            struct_ty,
            derefed_value.value_ptr,
            vec![
                self.context.get_i32(0).into(),
                self.context.get_i32(pos.unwrap() as u32).into(),
            ],
            None,
        );

        let ty = self.transform_interned_ty_faithfully(self.analyzer.get_expr_type(&extra.self_id));

        Some(ValuePtrContainer {
            value_ptr: v.into(),
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

        let v = self.builder.build_getelementptr(
            array_ty,
            derefed_value.value_ptr,
            vec![self.context.get_i32(0).into(), index_raw],
            None,
        );

        Some(ValuePtrContainer {
            value_ptr: v.into(),
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
        Some(self.get_value_by_index(index).into_normal().unwrap())
    }

    fn visit_addr_of_expr<'tmp>(
        &mut self,
        AddrOfExpr(_, inner_expr): &'ast AddrOfExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        let v = self.visit_expr(inner_expr, extra)?;
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

        let CycleInfo {
            continue_bb: _,
            next_bb,
            value,
        } = extra.cycle_info.unwrap();

        if let Some(v) = v {
            self.store_to_ptr(value.unwrap().clone(), v);
        };

        if let Some(e) = inner_expr {
            self.try_build_branch(next_bb.clone(), &e.id);
        } else {
            self.builder.build_branch(next_bb.clone());
        }

        None
    }

    fn visit_continue_expr<'tmp>(
        &mut self,
        ContinueExpr: &'ast ContinueExpr,
        extra: Self::ExprExtra<'tmp>,
    ) -> Self::ExprRes<'_> {
        let CycleInfo { continue_bb, .. } = extra.cycle_info.unwrap();

        self.builder.build_branch(continue_bb.clone());

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

        let value = self.build_alloca(ty.clone(), None);
        let indexes = self
            .analyzer
            .get_expr_value(&extra.self_id)
            .kind
            .as_struct()
            .unwrap();

        for (ExprField { expr, .. }, index) in zip(fields, indexes) {
            let v = self.visit_expr(expr, extra)?;
            let ith = self.builder.build_getelementptr(
                ty.clone(),
                value.clone().into(),
                vec![
                    self.context.get_i32(0).into(),
                    self.context.get_i32(*index as u32).into(),
                ],
                None,
            );
            self.store_to_ptr(ith.into(), v);
        }

        Some(ValuePtrContainer {
            value_ptr: value.into(),
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
        let value = self.build_alloca(ty.clone(), None);

        let current_function = self.builder.get_current_function().clone();
        let repeat_loop_header_bb = self
            .context
            .append_basic_block(&current_function, ".repeat_loop_header");
        let repeat_loop_body_bb = self
            .context
            .append_basic_block(&current_function, ".repeat_loop_body");
        let repeat_loop_next_bb = self
            .context
            .append_basic_block(&current_function, ".repeat_loop_next");
        let repeat_counter =
            self.build_alloca(self.context.i32_type().into(), Some(".repeat_counter"));
        self.builder.build_store(
            self.context.get_i32(0).into(),
            repeat_counter.clone().into(),
        );
        self.builder.build_branch(repeat_loop_header_bb.clone());

        // header
        self.builder
            .locate_end(current_function.clone(), repeat_loop_header_bb.clone());
        let repeat_counter_v = self.builder.build_load(
            self.context.i32_type().into(),
            repeat_counter.clone().into(),
            None,
        );
        let cond = self.builder.build_icmp(
            crate::ir::ir_value::ICmpCode::Eq,
            repeat_counter_v.clone().into(),
            self.context.get_i32(repeat_num).into(),
            None,
        );
        self.builder.build_conditional_branch(
            cond.into(),
            repeat_loop_next_bb.clone(),
            repeat_loop_body_bb.clone(),
        );

        // body
        self.builder
            .locate_end(current_function.clone(), repeat_loop_body_bb.clone());
        let ith_ptr = self.builder.build_getelementptr(
            inner_ty,
            value.clone().into(),
            vec![repeat_counter_v.clone().into()],
            None,
        );
        self.store_to_ptr(ith_ptr.into(), inner_value.clone());
        let new_counter_v = self.builder.build_binary(
            crate::ir::ir_value::BinaryOpcode::Add,
            self.context.i32_type().into(),
            repeat_counter_v.into(),
            self.context.get_i32(1).into(),
            None,
        );
        self.builder
            .build_store(new_counter_v.into(), repeat_counter.into());
        self.builder.build_branch(repeat_loop_header_bb.clone());

        // next
        self.builder
            .locate_end(current_function, repeat_loop_next_bb);

        Some(ValuePtrContainer {
            value_ptr: value.into(),
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
