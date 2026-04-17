use crate::mir::{BlockId, Register, StackSlotId, SymbolId, TargetArch, TargetInst};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RV32Arch;

impl TargetArch for RV32Arch {
    type PhysicalReg = RV32Reg;
    type MachineInst = RV32Inst;
}

#[rustfmt::skip]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum RV32Reg {
    Zero, Ra, Sp, Gp, Tp, 
    T0, T1, T2, S0, S1, 
    A0, A1, A2, A3, A4, A5, A6, A7, 
    S2, S3, S4, S5, S6, S7, S8, S9, S10, S11, 
    T3, T4, T5, T6,
}

impl RV32Reg {
    pub fn reg_a(index: usize) -> Self {
        match index {
            0 => RV32Reg::A0,
            1 => RV32Reg::A1,
            2 => RV32Reg::A2,
            3 => RV32Reg::A3,
            4 => RV32Reg::A4,
            5 => RV32Reg::A5,
            6 => RV32Reg::A6,
            7 => RV32Reg::A7,
            _ => panic!("Invalid register index"),
        }
    }
}

type Reg = Register<RV32Reg>;

#[rustfmt::skip]
#[derive(Debug, Clone)]
pub enum RV32Inst {
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
    Lbu { rd: Reg, rs1: Reg, imm: i32 },
    Lhu { rd: Reg, rs1: Reg, imm: i32 },

    Sb { rs1: Reg, rs2: Reg, imm: i32 },
    Sh { rs1: Reg, rs2: Reg, imm: i32 },
    Sw { rs1: Reg, rs2: Reg, imm: i32 },

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

    // Pseudo-instructions https://github.com/DarkSharpness/REIMU/blob/main/docs/support.md
    Mv { rd: Reg, rs: Reg },
    Li { rd: Reg, imm: i32 },

    Ret,
    La { rd: Reg, label: SymbolId },
    Nop,
    Lbs { rd: Reg, symbol: SymbolId },
    Lhs { rd: Reg, symbol: SymbolId },
    Lws { rd: Reg, symbol: SymbolId },
    Sbs { rs: Reg, symbol: SymbolId },
    Shs { rs: Reg, symbol: SymbolId },
    Sws { rs: Reg, symbol: SymbolId },

    Call { func: SymbolId, num_args: usize, stack_arg_size: usize },
    Tail { func: SymbolId, num_args: usize, stack_arg_size: usize },

    LoadStack { rd: Reg, slot: StackSlotId },
    SaveStack { rs: Reg, slot: StackSlotId },
}

impl TargetInst for RV32Inst {
    type PhysicalReg = RV32Reg;

    fn def_regs(&self) -> Vec<Register<Self::PhysicalReg>> {
        use RV32Inst::*;
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
            | Lbu { rd, .. }
            | Lhu { rd, .. }
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
            | Lws { rd, .. } => vec![*rd],
            Call { .. } => {
                vec![
                    Register::Physical(RV32Reg::Ra),
                    Register::Physical(RV32Reg::A0),
                    Register::Physical(RV32Reg::A1),
                    Register::Physical(RV32Reg::A2),
                    Register::Physical(RV32Reg::A3),
                    Register::Physical(RV32Reg::A4),
                    Register::Physical(RV32Reg::A5),
                    Register::Physical(RV32Reg::A6),
                    Register::Physical(RV32Reg::A7),
                    Register::Physical(RV32Reg::T0),
                    Register::Physical(RV32Reg::T1),
                    Register::Physical(RV32Reg::T2),
                    Register::Physical(RV32Reg::T3),
                    Register::Physical(RV32Reg::T4),
                    Register::Physical(RV32Reg::T5),
                    Register::Physical(RV32Reg::T6),
                ]
            }
            Tail { .. } => vec![],
            _ => vec![],
        }
    }

    fn use_regs(&self) -> Vec<Register<Self::PhysicalReg>> {
        use RV32Inst::*;
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
            | Lbu { rs1, .. }
            | Lhu { rs1, .. } => vec![*rs1],
            Sb { rs1, rs2, .. } | Sh { rs1, rs2, .. } | Sw { rs1, rs2, .. } => vec![*rs1, *rs2],
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
                Register::Physical(RV32Reg::Ra),
                Register::Physical(RV32Reg::A0),
                Register::Physical(RV32Reg::A1),
            ],
            Sbs { rs, .. } | Shs { rs, .. } | Sws { rs, .. } => vec![*rs],
            Call { num_args, .. } | Tail { num_args, .. } => [
                RV32Reg::A0,
                RV32Reg::A1,
                RV32Reg::A2,
                RV32Reg::A3,
                RV32Reg::A4,
                RV32Reg::A5,
                RV32Reg::A6,
                RV32Reg::A7,
            ][..(*num_args).min(8)]
                .into_iter()
                .map(|r| Register::Physical(*r))
                .collect(),
            _ => vec![],
        }
    }

    fn is_terminator(&self) -> bool {
        use RV32Inst::*;
        match self {
            Beq { .. }
            | Bne { .. }
            | Blt { .. }
            | Bge { .. }
            | Bltu { .. }
            | Bgeu { .. }
            | Jal { .. }
            | Jalr { .. }
            | Ret { .. }
            | Tail { .. } => true,
            _ => false,
        }
    }
}
