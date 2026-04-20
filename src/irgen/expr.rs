use std::iter::once;

use crate::{
    ast::{
        NodeId,
        expr::{BinOp, Expr},
    },
    impossible,
    ir::{
        core::ValueId,
        core_inst::{BinaryOpcode as CoreBinaryOpcode, ICmpCode as CoreICmpCode},
        ir_type::TypePtr,
        ir_value::{BasicBlockPtr, BinaryOpcode, ICmpCode, ValuePtr},
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
            let ret = self.build_alloca(string_ty.clone(), None);

            let func = self.module.get_function("string_plus").unwrap();
            let args = once(ret.clone().into())
                .chain(
                    vec![value1, value2]
                        .into_iter()
                        .flat_map(|x| self.get_value_presentation(x).flatten()),
                )
                .collect();
            self.builder.build_call(func, args, None);

            if let (Some(core_value1), Some(core_value2)) = (
                self.get_core_expr_value(&expr1.id),
                self.get_core_expr_value(&expr2.id),
            ) {
                let ret = self.build_core_alloca(string_ty.clone(), None);
                let func = self.core_module.borrow().get_function("string_plus").unwrap();
                let args = once(ret)
                    .chain(
                        vec![core_value1, core_value2]
                            .into_iter()
                            .flat_map(|x| self.core_get_value_presentation(x).flatten()),
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
            }

            Some(ValuePtrContainer {
                value_ptr: ret.into(),
                kind: ContainerKind::Ptr(string_ty),
            })
        } else {
            let raw1 = self.get_raw_value(value1);
            let raw2 = self.get_raw_value(value2);

            let intern = self.analyzer.get_expr_type(&extra.self_id);
            if let (Some(core_value1), Some(core_value2)) = (
                self.get_core_expr_value(&expr1.id),
                self.get_core_expr_value(&expr2.id),
            ) {
                let raw1 = self.core_get_raw_value(core_value1);
                let raw2 = self.core_get_raw_value(core_value2);
                let core_value = self.visit_binary_impl_core(bin_op, raw1, raw2, intern);
                self.set_core_expr_value(extra.self_id, core_value);
            }

            Some(self.visit_binary_impl(bin_op, raw1, raw2, intern))
        }
    }

    pub(crate) fn visit_binary_impl(
        &mut self,
        bin_op: BinOp,
        raw1: std::rc::Rc<crate::ir::ir_value::Value>,
        raw2: std::rc::Rc<crate::ir::ir_value::Value>,
        intern: crate::semantics::resolved_ty::TypeIntern,
    ) -> ValuePtrContainer {
        let resolved_ty = self.analyzer.probe_type(intern).unwrap();
        let is_signed = resolved_ty.is_signed_integer();
        let ty = self.transform_ty_faithfully(&resolved_ty);

        let op_code = match bin_op {
            BinOp::Add => BinaryOpcode::Add,
            BinOp::Sub => BinaryOpcode::Sub,
            BinOp::Mul => BinaryOpcode::Mul,
            BinOp::Div => {
                if is_signed {
                    BinaryOpcode::SDiv
                } else {
                    BinaryOpcode::UDiv
                }
            }
            BinOp::Rem => {
                if is_signed {
                    BinaryOpcode::SRem
                } else {
                    BinaryOpcode::URem
                }
            }
            BinOp::BitXor => BinaryOpcode::Xor,
            BinOp::BitAnd => BinaryOpcode::And,
            BinOp::BitOr => BinaryOpcode::Or,
            BinOp::Shl => BinaryOpcode::Shl,
            BinOp::Shr => {
                if is_signed {
                    BinaryOpcode::AShr
                } else {
                    BinaryOpcode::LShr
                }
            }
            _ => impossible!(),
        };

        let value = self.builder.build_binary(op_code, ty, raw1, raw2, None);

        ValuePtrContainer {
            value_ptr: value.into(),
            kind: ContainerKind::Raw { fat: None },
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

        let value = self.core_builder.build_binary(op_code, ty, raw1, raw2, None);

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
            BinOp::Eq => ICmpCode::Eq,
            BinOp::Lt => {
                if is_signed {
                    ICmpCode::Slt
                } else {
                    ICmpCode::Ult
                }
            }
            BinOp::Le => {
                if is_signed {
                    ICmpCode::Sle
                } else {
                    ICmpCode::Ule
                }
            }
            BinOp::Ne => ICmpCode::Ne,
            BinOp::Ge => {
                if is_signed {
                    ICmpCode::Sge
                } else {
                    ICmpCode::Uge
                }
            }
            BinOp::Gt => {
                if is_signed {
                    ICmpCode::Sgt
                } else {
                    ICmpCode::Ugt
                }
            }
            _ => impossible!(),
        };

        let value = self.builder.build_icmp(
            op_code,
            self.get_raw_value(value1),
            self.get_raw_value(value2),
            None,
        );

        if let (Some(core_value1), Some(core_value2)) = (
            self.get_core_expr_value(&expr1.id),
            self.get_core_expr_value(&expr2.id),
        ) {
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
            let lhs = self.core_get_raw_value(core_value1);
            let rhs = self.core_get_raw_value(core_value2);
            let value = self.core_builder.build_icmp(
                op_code,
                lhs,
                rhs,
                None,
            );
            self.set_core_expr_value(
                extra.self_id,
                CoreValueContainer {
                    value: ValueId::Inst(value),
                    kind: CoreContainerKind::Raw { fat: None },
                },
            );
        }

        Some(ValuePtrContainer {
            value_ptr: value.into(),
            kind: ContainerKind::Raw { fat: None },
        })
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

        let current_fn = self.builder.get_current_function().clone();
        let current_bb = self.builder.get_current_basic_block().clone();
        let right_bb = self.context.append_basic_block(&current_fn, ".right");
        let next_bb = self.context.append_basic_block(&current_fn, ".next");

        match bin_op {
            BinOp::And => self.try_build_conditional_branch(
                raw1.clone(),
                right_bb.clone(),
                next_bb.clone(),
                &expr1.id,
            ),
            BinOp::Or => self.try_build_conditional_branch(
                raw1.clone(),
                next_bb.clone(),
                right_bb.clone(),
                &expr1.id,
            ),
            _ => impossible!(),
        };

        self.builder
            .locate_end(current_fn.clone(), right_bb.clone());
        let value2 = self.visit_expr(expr2, extra)?;
        let raw2 = self.get_raw_value(value2);
        let new_right_bb = self.builder.get_current_basic_block().clone();
        self.try_build_branch(next_bb.clone(), &expr2.id);

        self.builder.locate_end(current_fn.clone(), next_bb.clone());
        let value = self.builder.build_phi(
            self.context.i1_type().into(),
            vec![(raw1, current_bb), (raw2, new_right_bb)],
            None,
        );

        Some(ValuePtrContainer {
            value_ptr: value.into(),
            kind: ContainerKind::Raw { fat: None },
        })
    }

    pub(crate) fn visit_ret_expr_impl(&mut self, inner_expr: Option<&'ast Expr>, extra: ExprExtra) {
        if let Some(e) = inner_expr {
            let v = self.visit_expr(e, extra);
            if let Some(v) = v {
                let v = if let Some(ret_ptr) = extra.ret_ptr {
                    self.store_to_ptr(ret_ptr.clone().into(), v);
                    None
                } else {
                    Some(self.get_value_presentation(v).value_ptr)
                };
                self.builder.build_return(v);
            }
        } else {
            self.builder.build_return(None);
        };
    }

    // 用于 branch 前检查是否会终止控制流，因为如果出现多余的 terminator，clang 会认为那是一个匿名基本块，从而破坏编号排名
    pub fn try_build_return(&self, value: Option<ValuePtr>, expr_id: &NodeId) {
        let result = self.analyzer.get_expr_result(expr_id);
        if result.interrupt.is_not() {
            self.builder.build_return(value);
        }
    }

    pub fn core_try_build_return(&mut self, value: Option<ValueId>, expr_id: &NodeId) {
        let result = self.analyzer.get_expr_result(expr_id);
        if result.interrupt.is_not() {
            self.core_builder.build_return(value);
        }
    }

    pub fn try_build_branch(&self, dest: BasicBlockPtr, expr_id: &NodeId) {
        let result = self.analyzer.get_expr_result(expr_id);
        if result.interrupt.is_not() {
            self.builder.build_branch(dest);
        }
    }

    pub fn try_build_conditional_branch(
        &self,
        cond: crate::ir::ir_value::ValuePtr,
        iftrue: BasicBlockPtr,
        ifelse: BasicBlockPtr,
        expr_id: &NodeId,
    ) {
        let result = self.analyzer.get_expr_result(expr_id);
        if result.interrupt.is_not() {
            self.builder.build_conditional_branch(cond, iftrue, ifelse);
        }
    }

    pub(crate) fn special_method_call(&self, kind: ValueKind) -> ValuePtrContainer {
        match kind {
            ValueKind::Normal(..) => impossible!(),
            ValueKind::LenMethod(len) => ValuePtrContainer {
                value_ptr: self.context.get_i32(len).into(),
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
