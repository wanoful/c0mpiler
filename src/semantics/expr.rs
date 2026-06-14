use std::ops::{Add, BitOr, Deref};

use enum_as_inner::EnumAsInner;

use crate::{
    ast::{Mutability, NodeId, Span, expr::Expr},
    make_semantic_error,
    semantics::{
        analyzer::SemanticAnalyzer,
        error::SemanticError,
        resolved_ty::TypeIntern,
        value::{Value, ValueIndex},
    },
};

#[derive(Debug, Clone, Copy)]
pub struct ExprExtra {
    pub(crate) target_ty: Option<TypeIntern>,
    pub(crate) scope_id: NodeId,
    pub(crate) self_id: NodeId,
    pub(crate) allow_i32_max: bool,

    #[allow(dead_code)]
    pub(crate) span: Span,
}

impl ExprExtra {
    pub fn replace_target(self, target_ty: TypeIntern) -> Self {
        Self {
            target_ty: Some(target_ty),
            ..self
        }
    }
}

#[derive(Debug)]
pub struct ExprResult {
    pub value_index: ValueIndex,
    pub assignee: AssigneeKind,
    pub interrupt: ControlFlowInterruptKind,
}

#[derive(Debug, Clone, Copy)]
pub enum AssigneeKind {
    Place(Mutability),
    Value,
    Only,
}

impl AssigneeKind {
    pub fn merge(self, rhs: Self) -> Result<Self, SemanticError> {
        match (self, rhs) {
            (AssigneeKind::Place(mutability1), AssigneeKind::Place(mutability2)) => {
                Ok(AssigneeKind::Place(mutability1 & mutability2))
            }

            (AssigneeKind::Value, AssigneeKind::Place(Mutability::Mut))
            | (AssigneeKind::Place(Mutability::Mut), AssigneeKind::Value)
            | (AssigneeKind::Value, AssigneeKind::Place(Mutability::Not))
            | (AssigneeKind::Place(Mutability::Not), AssigneeKind::Value)
            | (AssigneeKind::Value, AssigneeKind::Value) => Ok(AssigneeKind::Value),

            (AssigneeKind::Only, AssigneeKind::Place(Mutability::Mut))
            | (AssigneeKind::Place(Mutability::Mut), AssigneeKind::Only)
            | (AssigneeKind::Only, AssigneeKind::Only) => Ok(AssigneeKind::Only),

            (AssigneeKind::Only, AssigneeKind::Place(Mutability::Not))
            | (AssigneeKind::Place(Mutability::Not), AssigneeKind::Only) => {
                Err(make_semantic_error!(Immutable))
            }

            (AssigneeKind::Only, AssigneeKind::Value)
            | (AssigneeKind::Value, AssigneeKind::Only) => {
                Err(make_semantic_error!(AssigneeKindMismatch))
            }
        }
    }
}

#[derive(Debug, Clone, Copy, EnumAsInner)]
pub enum ControlFlowInterruptKind {
    Not,
    Loop,
    Return,
}

impl ControlFlowInterruptKind {
    pub fn out_of_cycle(&self) -> Self {
        match self {
            ControlFlowInterruptKind::Loop | ControlFlowInterruptKind::Not => {
                ControlFlowInterruptKind::Not
            }
            ControlFlowInterruptKind::Return => ControlFlowInterruptKind::Return,
        }
    }
}

impl Add for ControlFlowInterruptKind {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        match (&self, &rhs) {
            (ControlFlowInterruptKind::Not, _) => rhs,
            (_, _) => self,
        }
    }
}

impl BitOr for ControlFlowInterruptKind {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        match (self, rhs) {
            (ControlFlowInterruptKind::Not, _) | (_, ControlFlowInterruptKind::Not) => {
                ControlFlowInterruptKind::Not
            }
            (ControlFlowInterruptKind::Loop, _) | (_, ControlFlowInterruptKind::Loop) => {
                ControlFlowInterruptKind::Loop
            }
            (ControlFlowInterruptKind::Return, ControlFlowInterruptKind::Return) => {
                ControlFlowInterruptKind::Return
            }
        }
    }
}

impl<'ast> SemanticAnalyzer<'ast> {
    pub(crate) fn set_expr_value(&mut self, expr_id: NodeId, value: Value<'ast>) {
        let replace = self.expr_value.insert(expr_id, value);
        debug_assert!(replace.is_none());
    }

    pub(crate) fn set_expr_result(&mut self, expr_id: NodeId, result: ExprResult) {
        let replace = self.expr_results.insert(expr_id, result);
        debug_assert!(replace.is_none());
    }

    pub(crate) fn set_expr_value_and_result(
        &mut self,
        expr_id: NodeId,
        value: Value<'ast>,
        assignee: AssigneeKind,
        interrupt: ControlFlowInterruptKind,
    ) {
        let replace = self.expr_value.insert(expr_id, value);
        debug_assert!(replace.is_none());
        let replace = self.expr_results.insert(
            expr_id,
            ExprResult {
                value_index: ValueIndex::Expr(expr_id),
                assignee,
                interrupt,
            },
        );
        debug_assert!(replace.is_none());
    }

    pub(crate) fn get_expr_result(&self, expr_id: &NodeId) -> &ExprResult {
        self.expr_results.get(expr_id).unwrap()
    }

    pub(crate) fn get_expr_value(&self, expr_id: &NodeId) -> &Value<'ast> {
        let index = &self.get_expr_result(expr_id).value_index;
        self.get_value_by_index(index)
    }

    pub(crate) fn get_expr_type(&self, expr_id: &NodeId) -> TypeIntern {
        self.get_expr_value(expr_id).ty
    }

    pub(crate) fn get_expr_type_opt(&self, expr_id: &NodeId) -> Option<TypeIntern> {
        self.expr_results
            .get(expr_id)
            .map(|r| self.get_value_by_index(&r.value_index).ty)
    }

    pub(crate) fn merge_result_info<'a, I, E>(
        &self,
        iter: I,
    ) -> Result<(ControlFlowInterruptKind, AssigneeKind), SemanticError>
    where
        I: Iterator<Item = &'a E>,
        E: Deref<Target = Expr> + 'a,
    {
        let mut interrupt = ControlFlowInterruptKind::Not;
        let mut assignee = AssigneeKind::Place(Mutability::Mut);

        for e in iter {
            let result = self.get_expr_result(&e.id);
            interrupt = interrupt + result.interrupt;
            assignee = assignee.merge(result.assignee)?;
        }

        Ok((interrupt, assignee))
    }

    pub(crate) fn merge_expr_interrupt<'a, I, E>(&self, iter: I) -> ControlFlowInterruptKind
    where
        I: Iterator<Item = &'a E>,
        E: Deref<Target = Expr> + 'a,
    {
        let mut interrupt = ControlFlowInterruptKind::Not;

        for e in iter {
            let result = self.get_expr_result(&e.id);
            interrupt = interrupt + result.interrupt;
        }

        interrupt
    }

    pub(crate) fn batch_no_assignee_expr<'a, I, E>(&self, iter: I) -> Result<(), SemanticError>
    where
        I: Iterator<Item = &'a E>,
        E: Deref<Target = Expr> + 'a,
    {
        for e in iter {
            let result = self.get_expr_result(&e.id);
            if matches!(result.assignee, AssigneeKind::Only) {
                return Err(make_semantic_error!(AssigneeExprNotAllowed).set_span(&e.span));
            }
        }

        Ok(())
    }

    pub(crate) fn no_assignee(&self, id: NodeId) -> Result<(), SemanticError> {
        if matches!(self.get_expr_result(&id).assignee, AssigneeKind::Only) {
            Err(make_semantic_error!(AssigneeExprNotAllowed))
        } else {
            Ok(())
        }
    }
}
