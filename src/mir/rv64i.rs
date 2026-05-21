pub(crate) mod print;

use std::{collections::HashMap, ops::RangeInclusive};

use crate::{
    impossible,
    mir::{
        BlockId, FrameLayout, LoweringTarget, Register, StackSlotId, SymbolId, TargetArch,
        TargetInst, generate_reg_rewrite, rv64i::print::RV64InstPrinter,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RV64Arch;

impl TargetArch for RV64Arch {
    type PhysicalReg = RV64Reg;
    type MachineInst = RV64Inst;
    type InstPrinter<'a> = RV64InstPrinter<'a>;

    fn get_allocatable_regs() -> Vec<Self::PhysicalReg> {
        use RV64Reg::*;
        vec![
            T0, T1, T2, T3, T4, T5, S0, S1, S2, S3, S4, S5, S6, S7, S8, S9, S10, S11, A0, A1, A2,
            A3, A4, A5, A6, A7,
        ]
    }

    fn spill_scratch_regs() -> &'static [Self::PhysicalReg]
    where
        Self: Sized,
    {
        &[RV64Reg::T6]
    }

    fn is_callee_saved(reg: Self::PhysicalReg) -> bool {
        matches!(
            reg,
            RV64Reg::S0
                | RV64Reg::S1
                | RV64Reg::S2
                | RV64Reg::S3
                | RV64Reg::S4
                | RV64Reg::S5
                | RV64Reg::S6
                | RV64Reg::S7
                | RV64Reg::S8
                | RV64Reg::S9
                | RV64Reg::S10
                | RV64Reg::S11
        )
    }

    fn branch_offset_range() -> RangeInclusive<isize> {
        -4096..=4095
    }
}

impl Default for RV64Arch {
    fn default() -> Self {
        Self
    }
}

#[rustfmt::skip]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum RV64Reg {
    Zero, Ra, Sp, Gp, Tp,
    T0, T1, T2, S0, S1,
    A0, A1, A2, A3, A4, A5, A6, A7,
    S2, S3, S4, S5, S6, S7, S8, S9, S10, S11,
    T3, T4, T5, T6,
}

impl RV64Reg {
    pub fn reg_a(index: usize) -> Self {
        match index {
            0 => RV64Reg::A0,
            1 => RV64Reg::A1,
            2 => RV64Reg::A2,
            3 => RV64Reg::A3,
            4 => RV64Reg::A4,
            5 => RV64Reg::A5,
            6 => RV64Reg::A6,
            7 => RV64Reg::A7,
            _ => panic!("Invalid register index"),
        }
    }
}

type Reg = Register<RV64Reg>;

generate_reg_rewrite! {
#[rustfmt::skip]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RV64Inst {
    Add { rd: Reg, rs1: Reg, rs2: Reg },
    Sub { rd: Reg, rs1: Reg, rs2: Reg },
    Xor { rd: Reg, rs1: Reg, rs2: Reg },
    Or { rd: Reg, rs1: Reg, rs2: Reg },
    And { rd: Reg, rs1: Reg, rs2: Reg },
    Sll { rd: Reg, rs1: Reg, rs2: Reg },
    Srl { rd: Reg, rs1: Reg, rs2: Reg },
    Sra { rd: Reg, rs1: Reg, rs2: Reg },
    Slt { rd: Reg, rs1: Reg, rs2: Reg },
    Sltu { rd: Reg, rs1: Reg, rs2: Reg },

    Addi { rd: Reg, rs1: Reg, imm: i32 },
    Xori { rd: Reg, rs1: Reg, imm: i32 },
    Ori { rd: Reg, rs1: Reg, imm: i32 },
    Andi { rd: Reg, rs1: Reg, imm: i32 },
    Slli { rd: Reg, rs1: Reg, imm: i32 },
    Srli { rd: Reg, rs1: Reg, imm: i32 },
    Srai { rd: Reg, rs1: Reg, imm: i32 },
    Slti { rd: Reg, rs1: Reg, imm: i32 },
    Sltiu { rd: Reg, rs1: Reg, imm: i32 },

    Lb { rd: Reg, rs1: Reg, imm: i32 },
    Lh { rd: Reg, rs1: Reg, imm: i32 },
    Lw { rd: Reg, rs1: Reg, imm: i32 },
    Ld { rd: Reg, rs1: Reg, imm: i32 },
    Lbu { rd: Reg, rs1: Reg, imm: i32 },
    Lhu { rd: Reg, rs1: Reg, imm: i32 },
    Lwu { rd: Reg, rs1: Reg, imm: i32 },

    Sb { rs1: Reg, rs2: Reg, imm: i32 },
    Sh { rs1: Reg, rs2: Reg, imm: i32 },
    Sw { rs1: Reg, rs2: Reg, imm: i32 },
    Sd { rs1: Reg, rs2: Reg, imm: i32 },

    Beq { rs1: Reg, rs2: Reg, label: BlockId },
    Bne { rs1: Reg, rs2: Reg, label: BlockId },
    Blt { rs1: Reg, rs2: Reg, label: BlockId },
    Bge { rs1: Reg, rs2: Reg, label: BlockId },
    Bltu { rs1: Reg, rs2: Reg, label: BlockId },
    Bgeu { rs1: Reg, rs2: Reg, label: BlockId },

    Jal { rd: Reg, label: BlockId },
    Jalr { rd: Reg, rs1: Reg, imm: i32 },

    Lui { rd: Reg, imm: i32 },
    Auipc { rd: Reg, imm: i32 },

    Mul { rd: Reg, rs1: Reg, rs2: Reg },
    Mulh { rd: Reg, rs1: Reg, rs2: Reg },
    Mulsu { rd: Reg, rs1: Reg, rs2: Reg },
    Mulu { rd: Reg, rs1: Reg, rs2: Reg },
    Div { rd: Reg, rs1: Reg, rs2: Reg },
    Divu { rd: Reg, rs1: Reg, rs2: Reg },
    Rem { rd: Reg, rs1: Reg, rs2: Reg },
    Remu { rd: Reg, rs1: Reg, rs2: Reg },

    Mv { rd: Reg, rs: Reg },
    Li { rd: Reg, imm: i32 },

    Ret,
    La { rd: Reg, label: SymbolId },
    Nop,
    Lbs { rd: Reg, symbol: SymbolId },
    Lhs { rd: Reg, symbol: SymbolId },
    Lws { rd: Reg, symbol: SymbolId },
    Lds { rd: Reg, symbol: SymbolId },
    Sbs { rs: Reg, symbol: SymbolId, rt: Reg },
    Shs { rs: Reg, symbol: SymbolId, rt: Reg },
    Sws { rs: Reg, symbol: SymbolId, rt: Reg },
    Sds { rs: Reg, symbol: SymbolId, rt: Reg },

    Call { func: SymbolId, num_args: usize },
    Tail { func: SymbolId, num_args: usize },

    LoadStack { rd: Reg, slot: StackSlotId },
    SaveStack { rs: Reg, slot: StackSlotId, rt: Reg },
    StoreOutgoingArg { rs: Reg, offset: i32, rt: Reg },
    LoadIncomingArg { rd: Reg, offset: i32 },
    GetStackAddr { rd: Reg, slot: StackSlotId },
}
}

impl TargetInst for RV64Inst {
    type PhysicalReg = RV64Reg;

    fn def_regs(&self) -> Vec<Register<Self::PhysicalReg>> {
        use RV64Inst::*;
        match self {
            Add { rd, .. }
            | Sub { rd, .. }
            | Xor { rd, .. }
            | Or { rd, .. }
            | And { rd, .. }
            | Sll { rd, .. }
            | Srl { rd, .. }
            | Sra { rd, .. }
            | Slt { rd, .. }
            | Sltu { rd, .. }
            | Addi { rd, .. }
            | Xori { rd, .. }
            | Ori { rd, .. }
            | Andi { rd, .. }
            | Slli { rd, .. }
            | Srli { rd, .. }
            | Srai { rd, .. }
            | Slti { rd, .. }
            | Sltiu { rd, .. }
            | Lb { rd, .. }
            | Lh { rd, .. }
            | Lw { rd, .. }
            | Ld { rd, .. }
            | Lbu { rd, .. }
            | Lhu { rd, .. }
            | Lwu { rd, .. }
            | Jal { rd, .. }
            | Jalr { rd, .. }
            | Lui { rd, .. }
            | Auipc { rd, .. }
            | Mul { rd, .. }
            | Mulh { rd, .. }
            | Mulsu { rd, .. }
            | Mulu { rd, .. }
            | Div { rd, .. }
            | Divu { rd, .. }
            | Rem { rd, .. }
            | Remu { rd, .. } => vec![*rd],
            Mv { rd, .. }
            | Li { rd, .. }
            | La { rd, .. }
            | Lbs { rd, .. }
            | Lhs { rd, .. }
            | Lws { rd, .. }
            | Lds { rd, .. } => vec![*rd],
            LoadStack { rd, .. } | LoadIncomingArg { rd, .. } | GetStackAddr { rd, .. } => {
                vec![*rd]
            }
            Sbs { rt, .. }
            | Shs { rt, .. }
            | Sws { rt, .. }
            | Sds { rt, .. }
            | SaveStack { rt, .. }
            | StoreOutgoingArg { rt, .. } => vec![*rt],
            Call { .. } => {
                vec![
                    Register::Physical(RV64Reg::Ra),
                    Register::Physical(RV64Reg::A0),
                    Register::Physical(RV64Reg::A1),
                    Register::Physical(RV64Reg::A2),
                    Register::Physical(RV64Reg::A3),
                    Register::Physical(RV64Reg::A4),
                    Register::Physical(RV64Reg::A5),
                    Register::Physical(RV64Reg::A6),
                    Register::Physical(RV64Reg::A7),
                    Register::Physical(RV64Reg::T0),
                    Register::Physical(RV64Reg::T1),
                    Register::Physical(RV64Reg::T2),
                    Register::Physical(RV64Reg::T3),
                    Register::Physical(RV64Reg::T4),
                    Register::Physical(RV64Reg::T5),
                    Register::Physical(RV64Reg::T6),
                ]
            }
            Tail { .. } => vec![],
            _ => vec![],
        }
    }

    fn use_regs(&self) -> Vec<Register<Self::PhysicalReg>> {
        use RV64Inst::*;
        match self {
            Add { rs1, rs2, .. }
            | Sub { rs1, rs2, .. }
            | Xor { rs1, rs2, .. }
            | Or { rs1, rs2, .. }
            | And { rs1, rs2, .. }
            | Sll { rs1, rs2, .. }
            | Srl { rs1, rs2, .. }
            | Sra { rs1, rs2, .. }
            | Slt { rs1, rs2, .. }
            | Sltu { rs1, rs2, .. } => vec![*rs1, *rs2],
            Addi { rs1, .. }
            | Xori { rs1, .. }
            | Ori { rs1, .. }
            | Andi { rs1, .. }
            | Slli { rs1, .. }
            | Srli { rs1, .. }
            | Srai { rs1, .. }
            | Slti { rs1, .. }
            | Sltiu { rs1, .. } => vec![*rs1],
            Lb { rs1, .. }
            | Lh { rs1, .. }
            | Lw { rs1, .. }
            | Ld { rs1, .. }
            | Lbu { rs1, .. }
            | Lhu { rs1, .. }
            | Lwu { rs1, .. } => vec![*rs1],
            Sb { rs1, rs2, .. } | Sh { rs1, rs2, .. } | Sw { rs1, rs2, .. } | Sd { rs1, rs2, .. } => {
                vec![*rs1, *rs2]
            }
            Beq { rs1, rs2, .. }
            | Bne { rs1, rs2, .. }
            | Blt { rs1, rs2, .. }
            | Bge { rs1, rs2, .. }
            | Bltu { rs1, rs2, .. }
            | Bgeu { rs1, rs2, .. } => vec![*rs1, *rs2],
            Jalr { rs1, .. } => vec![*rs1],
            Mul { rs1, rs2, .. }
            | Mulh { rs1, rs2, .. }
            | Mulsu { rs1, rs2, .. }
            | Mulu { rs1, rs2, .. }
            | Div { rs1, rs2, .. }
            | Divu { rs1, rs2, .. }
            | Rem { rs1, rs2, .. }
            | Remu { rs1, rs2, .. } => vec![*rs1, *rs2],

            Mv { rs, .. } => vec![*rs],
            Ret => vec![
                Register::Physical(RV64Reg::Ra),
                Register::Physical(RV64Reg::A0),
                Register::Physical(RV64Reg::A1),
            ],
            Sbs { rs, .. } | Shs { rs, .. } | Sws { rs, .. } | Sds { rs, .. } => vec![*rs],
            SaveStack { rs, .. } | StoreOutgoingArg { rs, .. } => {
                vec![Register::Physical(RV64Reg::Sp), *rs]
            }
            LoadStack { .. } | LoadIncomingArg { .. } | GetStackAddr { .. } => {
                vec![Register::Physical(RV64Reg::Sp)]
            }
            Call { num_args, .. } | Tail { num_args, .. } => [
                RV64Reg::A0,
                RV64Reg::A1,
                RV64Reg::A2,
                RV64Reg::A3,
                RV64Reg::A4,
                RV64Reg::A5,
                RV64Reg::A6,
                RV64Reg::A7,
            ][..(*num_args).min(8)]
                .iter()
                .map(|r| Register::Physical(*r))
                .collect(),
            _ => vec![],
        }
    }

    fn is_terminator(&self) -> bool {
        use RV64Inst::*;
        matches!(
            self,
            Beq { .. }
                | Bne { .. }
                | Blt { .. }
                | Bge { .. }
                | Bltu { .. }
                | Bgeu { .. }
                | Jal { .. }
                | Jalr { .. }
                | Ret
                | Tail { .. }
        )
    }

    fn is_ret(&self) -> bool {
        matches!(self, RV64Inst::Ret)
    }

    fn load_imm(rd: Register<Self::PhysicalReg>, imm: i32) -> Self
    where
        Self: Sized,
    {
        RV64Inst::Li { rd, imm }
    }

    fn mv(rd: Register<Self::PhysicalReg>, rs: Register<Self::PhysicalReg>) -> Self
    where
        Self: Sized,
    {
        RV64Inst::Mv { rd, rs }
    }

    fn get_successors(&self) -> Vec<BlockId> {
        match self {
            RV64Inst::Beq { label, .. }
            | RV64Inst::Bne { label, .. }
            | RV64Inst::Blt { label, .. }
            | RV64Inst::Bge { label, .. }
            | RV64Inst::Bltu { label, .. }
            | RV64Inst::Bgeu { label, .. } => vec![*label],
            RV64Inst::Jal { label, .. } => vec![*label],
            RV64Inst::Jalr { .. } => impossible!(),
            RV64Inst::Ret => vec![],
            RV64Inst::Tail { .. } => vec![],
            _ => vec![],
        }
    }

    fn rewrite_vreg(
        &self,
        use_rewrites: &std::collections::HashMap<super::VRegId, Register<Self::PhysicalReg>>,
        def_rewrites: &std::collections::HashMap<super::VRegId, Register<Self::PhysicalReg>>,
    ) -> Self
    where
        Self: Sized,
    {
        self.rewrite_vreg(use_rewrites, def_rewrites)
    }

    fn is_call(&self) -> bool {
        matches!(self, RV64Inst::Call { .. })
    }

    fn size_in_bytes(&self) -> usize {
        use RV64Inst::*;

        match self {
            Call { .. } | Tail { .. } | La { .. } => 8,
            Lbs { .. } | Lhs { .. } | Lws { .. } | Lds { .. } | Sbs { .. } | Shs { .. }
            | Sws { .. } | Sds { .. } => 8,
            Li { imm, .. } => {
                if (-2048..=2047).contains(imm) {
                    4
                } else {
                    8
                }
            }
            _ => 4,
        }
    }

    fn get_branch_target(&mut self) -> Option<&mut BlockId> {
        match self {
            RV64Inst::Beq { label, .. }
            | RV64Inst::Bne { label, .. }
            | RV64Inst::Blt { label, .. }
            | RV64Inst::Bge { label, .. }
            | RV64Inst::Bltu { label, .. }
            | RV64Inst::Bgeu { label, .. } => Some(label),
            _ => None,
        }
    }

    fn def_conflict_regs(
        &self,
    ) -> std::collections::HashMap<Register<Self::PhysicalReg>, Vec<Register<Self::PhysicalReg>>>
    {
        match self {
            RV64Inst::SaveStack { rs, rt, .. } | RV64Inst::StoreOutgoingArg { rs, rt, .. } => {
                [(*rt, vec![*rs])].into_iter().collect()
            }
            _ => HashMap::new(),
        }
    }

    fn as_move(&self) -> Option<(Register<Self::PhysicalReg>, Register<Self::PhysicalReg>)> {
        match self {
            RV64Inst::Mv { rd, rs } => Some((*rd, *rs)),
            RV64Inst::Addi { rd, rs1, imm }
            | RV64Inst::Xori { rd, rs1, imm }
            | RV64Inst::Ori { rd, rs1, imm } => {
                if *imm == 0 {
                    Some((*rd, *rs1))
                } else {
                    None
                }
            }
            RV64Inst::Andi { rd, rs1, imm } => {
                if *imm == -1 {
                    Some((*rd, *rs1))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}

impl LoweringTarget for RV64Arch {
    const WORD_SIZE: usize = 8;
    const FRAME_ALIGN: usize = 16;
    const SHIFT_AMT_BITS: usize = 6;

    fn zero_reg() -> Self::PhysicalReg {
        RV64Reg::Zero
    }

    fn return_reg() -> Self::PhysicalReg {
        RV64Reg::A0
    }

    fn ra_reg() -> Self::PhysicalReg {
        RV64Reg::Ra
    }

    fn sp_reg() -> Self::PhysicalReg {
        RV64Reg::Sp
    }

    fn arg_reg(index: usize) -> Self::PhysicalReg {
        RV64Reg::reg_a(index)
    }

    fn num_arg_regs() -> usize {
        8
    }

    fn stack_arg_size() -> usize {
        8
    }

    fn stack_arg_offset(stack_index: usize) -> i32 {
        (stack_index * 8) as i32
    }

    fn emit_add(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV64Inst::Add { rd, rs1, rs2 }
    }

    fn emit_sub(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV64Inst::Sub { rd, rs1, rs2 }
    }

    fn emit_xor(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV64Inst::Xor { rd, rs1, rs2 }
    }

    fn emit_or(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV64Inst::Or { rd, rs1, rs2 }
    }

    fn emit_and(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV64Inst::And { rd, rs1, rs2 }
    }

    fn emit_sll(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV64Inst::Sll { rd, rs1, rs2 }
    }

    fn emit_srl(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV64Inst::Srl { rd, rs1, rs2 }
    }

    fn emit_sra(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV64Inst::Sra { rd, rs1, rs2 }
    }

    fn emit_slt(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV64Inst::Slt { rd, rs1, rs2 }
    }

    fn emit_sltu(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV64Inst::Sltu { rd, rs1, rs2 }
    }

    fn emit_mul(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV64Inst::Mul { rd, rs1, rs2 }
    }

    fn emit_div(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV64Inst::Div { rd, rs1, rs2 }
    }

    fn emit_divu(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV64Inst::Divu { rd, rs1, rs2 }
    }

    fn emit_rem(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV64Inst::Rem { rd, rs1, rs2 }
    }

    fn emit_remu(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV64Inst::Remu { rd, rs1, rs2 }
    }

    fn emit_addi(rd: Reg, rs1: Reg, imm: i32) -> Self::MachineInst {
        RV64Inst::Addi { rd, rs1, imm }
    }

    fn emit_xori(rd: Reg, rs1: Reg, imm: i32) -> Self::MachineInst {
        RV64Inst::Xori { rd, rs1, imm }
    }

    fn emit_ori(rd: Reg, rs1: Reg, imm: i32) -> Self::MachineInst {
        RV64Inst::Ori { rd, rs1, imm }
    }

    fn emit_andi(rd: Reg, rs1: Reg, imm: i32) -> Self::MachineInst {
        RV64Inst::Andi { rd, rs1, imm }
    }

    fn emit_slli(rd: Reg, rs1: Reg, imm: i32) -> Self::MachineInst {
        RV64Inst::Slli { rd, rs1, imm }
    }

    fn emit_srli(rd: Reg, rs1: Reg, imm: i32) -> Self::MachineInst {
        RV64Inst::Srli { rd, rs1, imm }
    }

    fn emit_srai(rd: Reg, rs1: Reg, imm: i32) -> Self::MachineInst {
        RV64Inst::Srai { rd, rs1, imm }
    }

    fn emit_sltiu(rd: Reg, rs1: Reg, imm: i32) -> Self::MachineInst {
        RV64Inst::Sltiu { rd, rs1, imm }
    }

    fn emit_branch_ne(rs1: Reg, rs2: Reg, label: BlockId) -> Self::MachineInst {
        RV64Inst::Bne { rs1, rs2, label }
    }

    fn emit_jump(label: BlockId) -> Self::MachineInst {
        RV64Inst::Jal {
            rd: Register::Physical(RV64Reg::Zero),
            label,
        }
    }

    fn emit_call(func: SymbolId, num_args: usize) -> Self::MachineInst {
        RV64Inst::Call { func, num_args }
    }

    fn emit_ret() -> Self::MachineInst {
        RV64Inst::Ret
    }

    fn emit_load_mem(
        rd: Reg,
        rs1: Reg,
        imm: i32,
        size: usize,
        unsigned: bool,
    ) -> Self::MachineInst {
        match (size, unsigned) {
            (1, false) => RV64Inst::Lb { rd, rs1, imm },
            (1, true) => RV64Inst::Lbu { rd, rs1, imm },
            (2, false) => RV64Inst::Lh { rd, rs1, imm },
            (2, true) => RV64Inst::Lhu { rd, rs1, imm },
            (4, false) => RV64Inst::Lw { rd, rs1, imm },
            (4, true) => RV64Inst::Lwu { rd, rs1, imm },
            (8, _) => RV64Inst::Ld { rd, rs1, imm },
            _ => panic!("unsupported load size"),
        }
    }

    fn emit_load_global(
        rd: Reg,
        symbol: SymbolId,
        size: usize,
        unsigned: bool,
    ) -> Self::MachineInst {
        match (size, unsigned) {
            (1, false) => RV64Inst::Lbs { rd, symbol },
            (2, false) => RV64Inst::Lhs { rd, symbol },
            (4, _) => RV64Inst::Lws { rd, symbol },
            (8, _) => RV64Inst::Lds { rd, symbol },
            _ => panic!("unsupported global load kind"),
        }
    }

    fn emit_load_symbol_addr(rd: Reg, symbol: SymbolId) -> Self::MachineInst {
        RV64Inst::La { rd, label: symbol }
    }

    fn emit_store_mem(rs1: Reg, rs2: Reg, imm: i32, size: usize) -> Self::MachineInst {
        match size {
            1 => RV64Inst::Sb { rs1, rs2, imm },
            2 => RV64Inst::Sh { rs1, rs2, imm },
            4 => RV64Inst::Sw { rs1, rs2, imm },
            8 => RV64Inst::Sd { rs1, rs2, imm },
            _ => panic!("unsupported store size"),
        }
    }

    fn emit_store_global(rs: Reg, symbol: SymbolId, size: usize, rt: Reg) -> Self::MachineInst {
        match size {
            1 => RV64Inst::Sbs { rs, symbol, rt },
            2 => RV64Inst::Shs { rs, symbol, rt },
            4 => RV64Inst::Sws { rs, symbol, rt },
            8 => RV64Inst::Sds { rs, symbol, rt },
            _ => panic!("unsupported global store kind"),
        }
    }

    fn emit_store_outgoing_arg(rs: Reg, offset: i32, rt: Reg) -> Self::MachineInst {
        RV64Inst::StoreOutgoingArg { rs, offset, rt }
    }

    fn emit_load_incoming_arg(rd: Reg, offset: i32) -> Self::MachineInst {
        RV64Inst::LoadIncomingArg { rd, offset }
    }

    fn emit_get_stack_addr(rd: Reg, slot: StackSlotId) -> Self::MachineInst {
        RV64Inst::GetStackAddr { rd, slot }
    }

    fn emit_load_stack_slot(
        rd: Register<Self::PhysicalReg>,
        slot: StackSlotId,
    ) -> Self::MachineInst {
        RV64Inst::LoadStack { rd, slot }
    }

    fn emit_store_stack_slot(
        rs: Register<Self::PhysicalReg>,
        slot: StackSlotId,
        rt: Register<Self::PhysicalReg>,
    ) -> Self::MachineInst {
        RV64Inst::SaveStack { rs, slot, rt }
    }

    fn emit_adjust_sp(offset: isize) -> Vec<Self::MachineInst> {
        if offset == 0 {
            vec![]
        } else if (-2048..=2047).contains(&offset) {
            vec![RV64Inst::Addi {
                rd: Register::Physical(RV64Reg::Sp),
                rs1: Register::Physical(RV64Reg::Sp),
                imm: offset as i32,
            }]
        } else {
            let temp_reg = Self::spill_scratch_regs()[0];

            vec![
                RV64Inst::Li {
                    rd: Register::Physical(temp_reg),
                    imm: offset as i32,
                },
                RV64Inst::Add {
                    rd: Register::Physical(RV64Reg::Sp),
                    rs1: Register::Physical(RV64Reg::Sp),
                    rs2: Register::Physical(temp_reg),
                },
            ]
        }
    }

    fn expand_pseudo(inst: &RV64Inst, frame_layout: &FrameLayout<RV64Arch>) -> Vec<RV64Inst>
    where
        Self: Sized,
    {
        use RV64Inst::*;
        match inst {
            LoadStack { rd, slot } => {
                let offset = frame_layout.slot_offsets[slot];
                if (-2048..=2047).contains(&offset) {
                    vec![RV64Inst::Ld {
                        rd: *rd,
                        rs1: Register::Physical(RV64Reg::Sp),
                        imm: offset as i32,
                    }]
                } else {
                    vec![
                        RV64Inst::Li {
                            rd: *rd,
                            imm: offset as i32,
                        },
                        RV64Inst::Add {
                            rd: *rd,
                            rs1: Register::Physical(RV64Reg::Sp),
                            rs2: *rd,
                        },
                        RV64Inst::Ld {
                            rd: *rd,
                            rs1: *rd,
                            imm: 0,
                        },
                    ]
                }
            }
            SaveStack { rs, slot, rt } => {
                let offset = frame_layout.slot_offsets[slot];
                if (-2048..=2047).contains(&offset) {
                    vec![RV64Inst::Sd {
                        rs1: Register::Physical(RV64Reg::Sp),
                        rs2: *rs,
                        imm: offset as i32,
                    }]
                } else {
                    vec![
                        RV64Inst::Li {
                            rd: *rt,
                            imm: offset as i32,
                        },
                        RV64Inst::Add {
                            rd: *rt,
                            rs1: Register::Physical(RV64Reg::Sp),
                            rs2: *rt,
                        },
                        RV64Inst::Sd {
                            rs1: *rt,
                            rs2: *rs,
                            imm: 0,
                        },
                    ]
                }
            }
            StoreOutgoingArg { rs, offset, rt } => {
                let offset = frame_layout.outgoing_arg_offset as i32 + *offset;
                if (-2048..=2047).contains(&offset) {
                    vec![RV64Inst::Sd {
                        rs1: Register::Physical(RV64Reg::Sp),
                        rs2: *rs,
                        imm: offset,
                    }]
                } else {
                    vec![
                        RV64Inst::Li {
                            rd: *rt,
                            imm: offset,
                        },
                        RV64Inst::Add {
                            rd: *rt,
                            rs1: Register::Physical(RV64Reg::Sp),
                            rs2: *rt,
                        },
                        RV64Inst::Sd {
                            rs1: *rt,
                            rs2: *rs,
                            imm: 0,
                        },
                    ]
                }
            }
            LoadIncomingArg { rd, offset } => {
                let offset = frame_layout.incoming_arg_offset as i32 + *offset;
                if (-2048..=2047).contains(&offset) {
                    vec![RV64Inst::Ld {
                        rd: *rd,
                        rs1: Register::Physical(RV64Reg::Sp),
                        imm: offset,
                    }]
                } else {
                    vec![
                        RV64Inst::Li {
                            rd: *rd,
                            imm: offset,
                        },
                        RV64Inst::Add {
                            rd: *rd,
                            rs1: Register::Physical(RV64Reg::Sp),
                            rs2: *rd,
                        },
                        RV64Inst::Ld {
                            rd: *rd,
                            rs1: *rd,
                            imm: 0,
                        },
                    ]
                }
            }
            GetStackAddr { rd, slot } => {
                let offset = frame_layout.slot_offsets[slot] as i32;
                if (-2048..=2047).contains(&offset) {
                    vec![RV64Inst::Addi {
                        rd: *rd,
                        rs1: Register::Physical(RV64Reg::Sp),
                        imm: offset,
                    }]
                } else {
                    vec![
                        RV64Inst::Li {
                            rd: *rd,
                            imm: offset,
                        },
                        RV64Inst::Add {
                            rd: *rd,
                            rs1: Register::Physical(RV64Reg::Sp),
                            rs2: *rd,
                        },
                    ]
                }
            }
            _ => vec![inst.clone()],
        }
    }

    fn is_jump_to(inst: &RV64Inst, target: BlockId) -> bool {
        match inst {
            RV64Inst::Jal {
                rd: Register::Physical(RV64Reg::Zero),
                label,
            } => *label == target,
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::mir;

    use super::*;

    #[test]
    fn test_rewrite() {
        let inst = RV64Inst::Add {
            rd: Register::Virtual(mir::VRegId(1)),
            rs1: Register::Virtual(mir::VRegId(2)),
            rs2: Register::Physical(RV64Reg::T0),
        };
        let mut use_rewrites = std::collections::HashMap::new();
        let mut def_rewrites = std::collections::HashMap::new();
        use_rewrites.insert(mir::VRegId(2), Register::Physical(RV64Reg::T1));
        def_rewrites.insert(mir::VRegId(1), Register::Physical(RV64Reg::T2));

        let rewritten = inst.rewrite_vreg(&use_rewrites, &def_rewrites);
        assert_eq!(
            rewritten,
            RV64Inst::Add {
                rd: Register::Physical(RV64Reg::T2),
                rs1: Register::Physical(RV64Reg::T1),
                rs2: Register::Physical(RV64Reg::T0),
            }
        );
    }
}
