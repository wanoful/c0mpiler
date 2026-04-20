use crate::ir::{
    core::{BlockId, FunctionId, ValueId},
    ir_type::TypePtr,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperandSlot {
    BinaryLhs,
    BinaryRhs,
    CallArg(usize),
    BranchCond,
    GEPBase,
    GEPIndex(usize),
    LoadPtr,
    RetVal,
    StorePtr,
    StoreVal,
    ICmpLhs,
    ICmpRhs,
    PhiIncomingVal(usize),
    SelectCond,
    SelectThenVal,
    SelectElseVal,
    PtrToIntPtr,
    TruncVal,
    ZextVal,
    SextVal,
}

#[derive(Debug, Clone)]
pub enum InstKind {
    Binary {
        op: BinaryOpcode,
        lhs: ValueId,
        rhs: ValueId,
    },
    Call {
        func: FunctionId,
        args: Vec<ValueId>,
    },
    Branch {
        then_block: BlockId,
        cond: Option<CondBranch>,
    },
    GetElementPtr {
        base_ty: TypePtr,
        base: ValueId,
        indices: Vec<ValueId>,
    },
    Alloca {
        ty: TypePtr,
    },
    Load {
        ptr: ValueId,
    },
    Ret {
        value: Option<ValueId>,
    },
    Store {
        value: ValueId,
        ptr: ValueId,
    },
    ICmp {
        op: ICmpCode,
        lhs: ValueId,
        rhs: ValueId,
    },
    Phi {
        incomings: Vec<PhiIncoming>,
    },
    Select {
        cond: ValueId,
        then_val: ValueId,
        else_val: ValueId,
    },
    PtrToInt {
        ptr: ValueId,
    },
    Trunc {
        value: ValueId,
    },
    Zext {
        value: ValueId,
    },
    Sext {
        value: ValueId,
    },
    Unreachable,
}

impl InstKind {
    pub fn is_terminator(&self) -> bool {
        matches!(
            self,
            InstKind::Branch { .. } | InstKind::Ret { .. } | InstKind::Unreachable
        )
    }

    pub fn is_phi(&self) -> bool {
        matches!(self, InstKind::Phi { .. })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct CondBranch {
    pub cond: ValueId,
    pub else_block: BlockId,
}

#[derive(Debug, Clone, Copy)]
pub struct PhiIncoming {
    pub block: BlockId,
    pub value: ValueId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOpcode {
    Add,
    Sub,
    Mul,
    UDiv,
    SDiv,
    URem,
    SRem,

    Shl,
    LShr,
    AShr,
    And,
    Or,
    Xor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ICmpCode {
    Eq,
    Ne,
    Ugt,
    Uge,
    Ult,
    Ule,
    Sgt,
    Sge,
    Slt,
    Sle,
}

impl InstKind {
    pub fn for_each_value_operand(&self, mut f: impl FnMut(ValueId, OperandSlot)) {
        match self {
            InstKind::Binary { lhs, rhs, .. } => {
                f(*lhs, OperandSlot::BinaryLhs);
                f(*rhs, OperandSlot::BinaryRhs)
            }
            InstKind::Call { args, .. } => args
                .iter()
                .enumerate()
                .for_each(|(i, arg)| f(*arg, OperandSlot::CallArg(i))),
            InstKind::Branch { cond, .. } => {
                if let Some(cond) = cond {
                    f(cond.cond, OperandSlot::BranchCond);
                }
            }
            InstKind::GetElementPtr { base, indices, .. } => {
                f(*base, OperandSlot::GEPBase);
                indices
                    .iter()
                    .enumerate()
                    .for_each(|(i, idx)| f(*idx, OperandSlot::GEPIndex(i)));
            }
            InstKind::Alloca { .. } => {}
            InstKind::Load { ptr } => f(*ptr, OperandSlot::LoadPtr),
            InstKind::Ret { value } => {
                if let Some(value) = value {
                    f(*value, OperandSlot::RetVal);
                }
            }
            InstKind::Store { value, ptr } => {
                f(*value, OperandSlot::StoreVal);
                f(*ptr, OperandSlot::StorePtr);
            }
            InstKind::ICmp { lhs, rhs, .. } => {
                f(*lhs, OperandSlot::ICmpLhs);
                f(*rhs, OperandSlot::ICmpRhs);
            }
            InstKind::Phi { incomings } => incomings
                .iter()
                .enumerate()
                .for_each(|(i, incoming)| f(incoming.value, OperandSlot::PhiIncomingVal(i))),
            InstKind::Select {
                cond,
                then_val,
                else_val,
            } => {
                f(*cond, OperandSlot::SelectCond);
                f(*then_val, OperandSlot::SelectThenVal);
                f(*else_val, OperandSlot::SelectElseVal);
            }
            InstKind::PtrToInt { ptr } => f(*ptr, OperandSlot::PtrToIntPtr),
            InstKind::Trunc { value } => f(*value, OperandSlot::TruncVal),
            InstKind::Zext { value } => f(*value, OperandSlot::ZextVal),
            InstKind::Sext { value } => f(*value, OperandSlot::SextVal),
            InstKind::Unreachable => {}
        }
    }

    pub fn replace_operand(&mut self, slot: OperandSlot, new_value: ValueId) {
        match (self, slot) {
            (InstKind::Binary { lhs, .. }, OperandSlot::BinaryLhs) => *lhs = new_value,
            (InstKind::Binary { rhs, .. }, OperandSlot::BinaryRhs) => *rhs = new_value,
            (InstKind::Call { args, .. }, OperandSlot::CallArg(i)) => args[i] = new_value,
            (
                InstKind::Branch {
                    cond: Some(cond), ..
                },
                OperandSlot::BranchCond,
            ) => cond.cond = new_value,
            (InstKind::GetElementPtr { base, .. }, OperandSlot::GEPBase) => *base = new_value,
            (InstKind::GetElementPtr { indices, .. }, OperandSlot::GEPIndex(i)) => {
                indices[i] = new_value
            }
            (InstKind::Load { ptr }, OperandSlot::LoadPtr) => *ptr = new_value,
            (InstKind::Ret { value: Some(value) }, OperandSlot::RetVal) => *value = new_value,
            (InstKind::Store { ptr, .. }, OperandSlot::StorePtr) => *ptr = new_value,
            (InstKind::Store { value, .. }, OperandSlot::StoreVal) => *value = new_value,
            (InstKind::ICmp { lhs, .. }, OperandSlot::ICmpLhs) => *lhs = new_value,
            (InstKind::ICmp { rhs, .. }, OperandSlot::ICmpRhs) => *rhs = new_value,
            (InstKind::Phi { incomings, .. }, OperandSlot::PhiIncomingVal(i)) => {
                incomings[i].value = new_value
            }
            (InstKind::Select { cond, .. }, OperandSlot::SelectCond) => *cond = new_value,
            (InstKind::Select { then_val, .. }, OperandSlot::SelectThenVal) => {
                *then_val = new_value
            }
            (InstKind::Select { else_val, .. }, OperandSlot::SelectElseVal) => {
                *else_val = new_value
            }
            (InstKind::PtrToInt { ptr }, OperandSlot::PtrToIntPtr) => *ptr = new_value,
            (InstKind::Trunc { value }, OperandSlot::TruncVal) => *value = new_value,
            (InstKind::Zext { value }, OperandSlot::ZextVal) => *value = new_value,
            (InstKind::Sext { value }, OperandSlot::SextVal) => *value = new_value,
            _ => panic!("Invalid operand slot for instruction kind"),
        }
    }
}
