use crate::mir::{Register, TargetInst};

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

    Beq { rs1: Reg, rs2: Reg, label: String },
    Bne { rs1: Reg, rs2: Reg, label: String },
    Blt { rs1: Reg, rs2: Reg, label: String },
    Bge { rs1: Reg, rs2: Reg, label: String },
    Bltu { rs1: Reg, rs2: Reg, label: String },
    Bgeu { rs1: Reg, rs2: Reg, label: String },

    Jal { rd: Reg, label: String },
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
    Neg { rd: Reg, rs: Reg },
    Not { rd: Reg, rs: Reg },
    Seqz { rd: Reg, rs: Reg },
    Snez { rd: Reg, rs: Reg },
    Sgtz { rd: Reg, rs: Reg },
    Sltz { rd: Reg, rs: Reg },
    Bgez { rs: Reg, label: String },
    Blez { rs: Reg, label: String },
    Bgtz { rs: Reg, label: String },
    Bltz { rs: Reg, label: String },
    Bnez { rs: Reg, label: String },
    Beqz { rs: Reg, label: String },
    Bgt { rs1: Reg, rs2: Reg, label: String },
    Ble { rs1: Reg, rs2: Reg, label: String },
    Bgtu { rs1: Reg, rs2: Reg, label: String },
    Bleu { rs1: Reg, rs2: Reg, label: String },

    J { label: String },
    Jal1 { label: String },
    Jr { rs: Reg },
    Jalr1 { rs: Reg },
    Ret,
    La { rd: Reg, label: String },
    Nop,
    Lbs { rd: Reg, symbol: String },
    Lhs { rd: Reg, symbol: String },
    Lws { rd: Reg, symbol: String },
    Sbs { rs: Reg, symbol: String },
    Shs { rs: Reg, symbol: String },
    Sws { rs: Reg, symbol: String },

    Call { func: String, num_args: usize },
    Tail { func: String, num_args: usize },
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
            | Neg { rd, .. }
            | Not { rd, .. }
            | Seqz { rd, .. }
            | Snez { rd, .. }
            | Sgtz { rd, .. }
            | Sltz { rd, .. }
            | La { rd, .. }
            | Lbs { rd, .. }
            | Lhs { rd, .. }
            | Lws { rd, .. } => vec![*rd],
            Jal1 { .. } | Jalr1 { .. } => vec![Register::Physical(RV32Reg::Ra)],
            Call { .. } | Tail { .. } => {
                let mut reg = vec![
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
                ];
                if matches!(self, Call { .. }) {
                    reg.push(Register::Physical(RV32Reg::Ra));
                };
                reg
            }
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

            Mv { rs, .. }
            | Neg { rs, .. }
            | Not { rs, .. }
            | Seqz { rs, .. }
            | Snez { rs, .. }
            | Sgtz { rs, .. }
            | Sltz { rs, .. } => vec![*rs],
            Bgez { rs, .. }
            | Blez { rs, .. }
            | Bgtz { rs, .. }
            | Bltz { rs, .. }
            | Bnez { rs, .. }
            | Beqz { rs, .. } => vec![*rs],
            Bgt { rs1, rs2, .. }
            | Ble { rs1, rs2, .. }
            | Bgtu { rs1, rs2, .. }
            | Bleu { rs1, rs2, .. } => vec![*rs1, *rs2],
            Jr { rs } => vec![*rs],
            Jalr1 { rs } => vec![*rs],
            Ret => vec![Register::Physical(RV32Reg::Ra)],
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
            | Bgez { .. }
            | Blez { .. }
            | Bgtz { .. }
            | Bltz { .. }
            | Bnez { .. }
            | Beqz { .. }
            | Bgt { .. }
            | Ble { .. }
            | Bgtu { .. }
            | Bleu { .. }
            | J { .. }
            | Jal1 { .. }
            | Jr { .. }
            | Jalr1 { .. }
            | Ret { .. }
            | Tail { .. } => true,
            _ => false,
        }
    }
}
