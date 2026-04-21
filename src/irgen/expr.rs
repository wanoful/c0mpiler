use std::iter::once;

use crate::{
    ast::{
        NodeId,
        expr::{BinOp, Expr},
    },
    impossible,
    ir::{
        core::{BlockRef, ValueId},
        core_inst::{BinaryOpcode as CoreBinaryOpcode, ICmpCode as CoreICmpCode},
        ir_type::TypePtr,
    },
    irgen::{
        IRGenerator,
        extra::ExprExtra,
        value::{
            ContainerKind, CoreContainerKind, CoreValueContainer, CoreValueKind, ValueKind,
            ValuePtrContainer,
        },
    },
    semantics::{analyzer::SemanticAnalyzer, visitor::Visitor},
};

impl<'ast, 'analyzer> IRGenerator<'ast, 'analyzer> {
    pub(crate) fn visit_binary(
        &mut self,
        bin_op: BinOp,
        expr1: &'ast Expr,
        expr2: &'ast Expr,
        extra: ExprExtra,
    ) -> Option<ValuePtrContainer> {
        let self_intern = self.analyzer.get_expr_type(&extra.self_id);
        let self_probe = self.analyzer.probe_type(self_intern).unwrap();
        let value1 = self.visit_expr(expr1, extra)?;
        let value2 = self.visit_expr(expr2, extra)?;
        if SemanticAnalyzer::is_string_type(&self_probe) {
            let string_ty: TypePtr = self.context.get_named_struct_type("String").unwrap().into();
            let ret = self.build_core_alloca(string_ty.clone(), None);
            let func = self
                .core_module
                .borrow()
                .get_function("string_plus")
                .unwrap();
            let args = once(ret)
                .chain(
                    vec![value1, value2]
                        .into_iter()
                        .flat_map(|x| self.get_value_presentation(x).flatten()),
                )
                .collect();
            self.core_builder.build_call(func, args, None);
            self.set_core_expr_value(
                extra.self_id,
                CoreValueContainer {
                    value: ret,
                    kind: CoreContainerKind::Ptr(string_ty.clone()),
                },
            );

            Some(ValuePtrContainer {
                value_ptr: ret,
                kind: ContainerKind::Ptr(string_ty),
            })
        } else {
            let raw1 = self.get_raw_value(value1);
            let raw2 = self.get_raw_value(value2);

            let intern = self.analyzer.get_expr_type(&extra.self_id);
            let value = self.visit_binary_impl_core(bin_op, raw1, raw2, intern);
            self.set_core_expr_value(extra.self_id, value.clone());
            Some(value.into())
        }
    }

    pub(crate) fn visit_binary_impl_core(
        &mut self,
        bin_op: BinOp,
        raw1: ValueId,
        raw2: ValueId,
        intern: crate::semantics::resolved_ty::TypeIntern,
    ) -> CoreValueContainer {
        let resolved_ty = self.analyzer.probe_type(intern).unwrap();
        let is_signed = resolved_ty.is_signed_integer();
        let ty = self.transform_ty_faithfully(&resolved_ty);

        let op_code = match bin_op {
            BinOp::Add => CoreBinaryOpcode::Add,
            BinOp::Sub => CoreBinaryOpcode::Sub,
            BinOp::Mul => CoreBinaryOpcode::Mul,
            BinOp::Div => {
                if is_signed {
                    CoreBinaryOpcode::SDiv
                } else {
                    CoreBinaryOpcode::UDiv
                }
            }
            BinOp::Rem => {
                if is_signed {
                    CoreBinaryOpcode::SRem
                } else {
                    CoreBinaryOpcode::URem
                }
            }
            BinOp::BitXor => CoreBinaryOpcode::Xor,
            BinOp::BitAnd => CoreBinaryOpcode::And,
            BinOp::BitOr => CoreBinaryOpcode::Or,
            BinOp::Shl => CoreBinaryOpcode::Shl,
            BinOp::Shr => {
                if is_signed {
                    CoreBinaryOpcode::AShr
                } else {
                    CoreBinaryOpcode::LShr
                }
            }
            _ => impossible!(),
        };

        let value = self
            .core_builder
            .build_binary(op_code, ty, raw1, raw2, None);

        CoreValueContainer {
            value: ValueId::Inst(value),
            kind: CoreContainerKind::Raw { fat: None },
        }
    }

    pub(crate) fn visit_compare(
        &mut self,
        bin_op: BinOp,
        expr1: &'ast Expr,
        expr2: &'ast Expr,
        extra: ExprExtra,
    ) -> Option<ValuePtrContainer> {
        let value1 = self.visit_expr(expr1, extra)?;
        let value2 = self.visit_expr(expr2, extra)?;

        let intern1 = self.analyzer.get_expr_type(&expr1.id);
        let resolved_ty1 = self.analyzer.probe_type(intern1).unwrap();
        let is_signed = resolved_ty1.is_signed_integer();

        let op_code = match bin_op {
            BinOp::Eq => CoreICmpCode::Eq,
            BinOp::Lt => {
                if is_signed {
                    CoreICmpCode::Slt
                } else {
                    CoreICmpCode::Ult
                }
            }
            BinOp::Le => {
                if is_signed {
                    CoreICmpCode::Sle
                } else {
                    CoreICmpCode::Ule
                }
            }
            BinOp::Ne => CoreICmpCode::Ne,
            BinOp::Ge => {
                if is_signed {
                    CoreICmpCode::Sge
                } else {
                    CoreICmpCode::Uge
                }
            }
            BinOp::Gt => {
                if is_signed {
                    CoreICmpCode::Sgt
                } else {
                    CoreICmpCode::Ugt
                }
            }
            _ => impossible!(),
        };

        let lhs = self.get_raw_value(value1);
        let rhs = self.get_raw_value(value2);
        let value = self.core_builder.build_icmp(op_code, lhs, rhs, None);
        let value = CoreValueContainer {
            value: ValueId::Inst(value),
            kind: CoreContainerKind::Raw { fat: None },
        };
        self.set_core_expr_value(extra.self_id, value.clone());
        Some(value.into())
    }

    pub(crate) fn visit_logic(
        &mut self,
        bin_op: BinOp,
        expr1: &'ast Expr,
        expr2: &'ast Expr,
        extra: ExprExtra,
    ) -> Option<ValuePtrContainer> {
        let value1 = self.visit_expr(expr1, extra)?;
        let raw1 = self.get_raw_value(value1);
        let current_fn = self.core_builder.get_current_function();
        let current_bb = self.core_builder.get_current_basic_block();
        let right_bb = self.core_builder.append_block(current_fn, Some(".right"));
        let next_bb = self.core_builder.append_block(current_fn, Some(".next"));

        match bin_op {
            BinOp::And => self.try_build_conditional_branch(raw1, right_bb, next_bb, &expr1.id),
            BinOp::Or => self.try_build_conditional_branch(raw1, next_bb, right_bb, &expr1.id),
            _ => impossible!(),
        };
        self.core_builder.locate_end(current_fn, right_bb);
        let value2 = self.visit_expr(expr2, extra)?;
        let raw2 = self.get_raw_value(value2);
        let new_right_bb = self.core_builder.get_current_basic_block();
        self.try_build_branch(next_bb, &expr2.id);

        self.core_builder.locate_end(current_fn, next_bb);
        let value = self.core_builder.build_phi(
            self.context.i1_type().into(),
            vec![(raw1, current_bb), (raw2, new_right_bb)],
            None,
        );
        let value = CoreValueContainer {
            value: ValueId::Inst(value),
            kind: CoreContainerKind::Raw { fat: None },
        };
        self.set_core_expr_value(extra.self_id, value.clone());
        Some(value.into())
    }

    pub(crate) fn visit_ret_expr_impl(&mut self, inner_expr: Option<&'ast Expr>, extra: ExprExtra) {
        if let Some(e) = inner_expr {
            let v = self.visit_expr(e, extra);
            if let Some(v) = v {
                let v = if let Some(ret_ptr) = extra.core_ret_ptr {
                    self.store_to_ptr(ret_ptr, v);
                    None
                } else {
                    Some(self.get_raw_value(v))
                };
                self.core_builder.build_return(v);
            }
        } else {
            self.core_builder.build_return(None);
        };
    }

    // 用于 branch 前检查是否会终止控制流，因为如果出现多余的 terminator，clang 会认为那是一个匿名基本块，从而破坏编号排名
    pub fn try_build_return(&mut self, value: Option<ValueId>, expr_id: &NodeId) {
        let result = self.analyzer.get_expr_result(expr_id);
        if result.interrupt.is_not() {
            self.core_builder.build_return(value);
        }
    }

    pub fn core_try_build_return(&mut self, value: Option<ValueId>, expr_id: &NodeId) {
        self.try_build_return(value, expr_id);
    }

    pub fn try_build_branch(&mut self, dest: BlockRef, expr_id: &NodeId) {
        let result = self.analyzer.get_expr_result(expr_id);
        if result.interrupt.is_not() {
            self.core_builder.build_branch(dest);
        }
    }

    pub fn core_try_build_branch(&mut self, dest: BlockRef, expr_id: &NodeId) {
        self.try_build_branch(dest, expr_id);
    }

    pub fn try_build_conditional_branch(
        &mut self,
        cond: ValueId,
        iftrue: BlockRef,
        ifelse: BlockRef,
        expr_id: &NodeId,
    ) {
        let result = self.analyzer.get_expr_result(expr_id);
        if result.interrupt.is_not() {
            self.core_builder
                .build_conditional_branch(cond, iftrue, ifelse);
        }
    }

    pub fn core_try_build_conditional_branch(
        &mut self,
        cond: ValueId,
        iftrue: BlockRef,
        ifelse: BlockRef,
        expr_id: &NodeId,
    ) {
        self.try_build_conditional_branch(cond, iftrue, ifelse, expr_id);
    }

    pub(crate) fn special_method_call(&mut self, kind: ValueKind) -> ValuePtrContainer {
        match kind {
            ValueKind::Normal(..) => impossible!(),
            ValueKind::LenMethod(len) => ValuePtrContainer {
                value_ptr: ValueId::Const(self.core_module.borrow_mut().add_i32_const(len)),
                kind: ContainerKind::Raw { fat: None },
            },
        }
    }

    pub(crate) fn special_method_call_core(&mut self, kind: CoreValueKind) -> CoreValueContainer {
        match kind {
            CoreValueKind::Normal(..) => impossible!(),
            CoreValueKind::LenMethod(len) => CoreValueContainer {
                value: ValueId::Const(self.core_module.borrow_mut().add_i32_const(len)),
                kind: CoreContainerKind::Raw { fat: None },
            },
        }
    }
}
