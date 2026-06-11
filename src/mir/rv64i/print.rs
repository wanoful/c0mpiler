use std::fmt::Display;

use crate::mir::{
    BlockId, SymbolId,
    print::InstPrinter,
    rv64i::{RV64Arch, RV64Inst, RV64Reg},
};

impl Display for RV64Reg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use RV64Reg::*;
        let name = match self {
            Zero => "zero",
            Ra => "ra",
            Sp => "sp",
            Gp => "gp",
            Tp => "tp",
            T0 => "t0",
            T1 => "t1",
            T2 => "t2",
            S0 => "s0",
            S1 => "s1",
            A0 => "a0",
            A1 => "a1",
            A2 => "a2",
            A3 => "a3",
            A4 => "a4",
            A5 => "a5",
            A6 => "a6",
            A7 => "a7",
            S2 => "s2",
            S3 => "s3",
            S4 => "s4",
            S5 => "s5",
            S6 => "s6",
            S7 => "s7",
            S8 => "s8",
            S9 => "s9",
            S10 => "s10",
            S11 => "s11",
            T3 => "t3",
            T4 => "t4",
            T5 => "t5",
            T6 => "t6",
        };
        write!(f, "{name}")
    }
}

pub struct RV64InstPrinter<'a> {
    inst: &'a RV64Inst,
    block_names: &'a std::collections::HashMap<BlockId, String>,
    symbol_names: &'a std::collections::HashMap<SymbolId, String>,
}

impl<'a> InstPrinter<'a, RV64Arch> for RV64InstPrinter<'a> {
    fn new(
        inst: &'a <RV64Arch as crate::mir::TargetArch>::MachineInst,
        block_names: &'a std::collections::HashMap<BlockId, String>,
        symbol_names: &'a std::collections::HashMap<SymbolId, String>,
    ) -> Self {
        Self {
            inst,
            block_names,
            symbol_names,
        }
    }
}

impl<'a> Display for RV64InstPrinter<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        use RV64Inst::*;
        match self.inst {
            Add { rd, rs1, rs2 }
            | Sub { rd, rs1, rs2 }
            | Addw { rd, rs1, rs2 }
            | Subw { rd, rs1, rs2 }
            | Xor { rd, rs1, rs2 }
            | Or { rd, rs1, rs2 }
            | And { rd, rs1, rs2 }
            | Sll { rd, rs1, rs2 }
            | Srl { rd, rs1, rs2 }
            | Sra { rd, rs1, rs2 }
            | Sllw { rd, rs1, rs2 }
            | Srlw { rd, rs1, rs2 }
            | Sraw { rd, rs1, rs2 }
            | Slt { rd, rs1, rs2 }
            | Sltu { rd, rs1, rs2 }
            | Mul { rd, rs1, rs2 }
            | Mulh { rd, rs1, rs2 }
            | Mulsu { rd, rs1, rs2 }
            | Mulu { rd, rs1, rs2 }
            | Mulw { rd, rs1, rs2 }
            | Div { rd, rs1, rs2 }
            | Divu { rd, rs1, rs2 }
            | Rem { rd, rs1, rs2 }
            | Remu { rd, rs1, rs2 }
            | Divw { rd, rs1, rs2 }
            | Divuw { rd, rs1, rs2 }
            | Remw { rd, rs1, rs2 }
            | Remuw { rd, rs1, rs2 } => {
                write!(f, "{} {}, {}, {}", self.inst.variant_name(), rd, rs1, rs2)
            }
            Addi { rd, rs1, imm }
            | Addiw { rd, rs1, imm }
            | Xori { rd, rs1, imm }
            | Ori { rd, rs1, imm }
            | Andi { rd, rs1, imm }
            | Slli { rd, rs1, imm }
            | Srli { rd, rs1, imm }
            | Srai { rd, rs1, imm }
            | Slliw { rd, rs1, imm }
            | Srliw { rd, rs1, imm }
            | Sraiw { rd, rs1, imm }
            | Slti { rd, rs1, imm }
            | Sltiu { rd, rs1, imm } => {
                write!(f, "{} {}, {}, {}", self.inst.variant_name(), rd, rs1, imm)
            }
            Lb { rd, rs1, imm }
            | Lh { rd, rs1, imm }
            | Lw { rd, rs1, imm }
            | Ld { rd, rs1, imm }
            | Lbu { rd, rs1, imm }
            | Lhu { rd, rs1, imm }
            | Lwu { rd, rs1, imm }
            | Jalr { rd, rs1, imm } => {
                write!(f, "{} {rd}, {imm}({rs1})", self.inst.variant_name())
            }
            Sb { rs1, rs2, imm }
            | Sh { rs1, rs2, imm }
            | Sw { rs1, rs2, imm }
            | Sd { rs1, rs2, imm } => {
                write!(f, "{} {rs2}, {imm}({rs1})", self.inst.variant_name())
            }
            Beq { rs1, rs2, label }
            | Bne { rs1, rs2, label }
            | Blt { rs1, rs2, label }
            | Bge { rs1, rs2, label }
            | Bltu { rs1, rs2, label }
            | Bgeu { rs1, rs2, label } => {
                let label_name = &self.block_names[label];
                write!(
                    f,
                    "{} {}, {}, {}",
                    self.inst.variant_name(),
                    rs1,
                    rs2,
                    label_name
                )
            }
            Jal { rd, label } => {
                let label_name = &self.block_names[label];
                write!(f, "{} {}, {}", self.inst.variant_name(), rd, label_name)
            }

            Lui { rd, imm } | Auipc { rd, imm } => {
                write!(f, "{} {}, {}", self.inst.variant_name(), rd, imm)
            }

            Mv { rd, rs } => write!(f, "{} {}, {}", self.inst.variant_name(), rd, rs),
            Li { rd, imm } => write!(f, "{} {}, {}", self.inst.variant_name(), rd, imm),
            Ret => write!(f, "{}", self.inst.variant_name()),
            La { rd, label } => {
                let symbol_name = &self.symbol_names[label];
                write!(f, "{} {}, {}", self.inst.variant_name(), rd, symbol_name)
            }
            Nop => write!(f, "{}", self.inst.variant_name()),
            Lbs { rd, symbol } | Lhs { rd, symbol } | Lws { rd, symbol } | Lds { rd, symbol } => {
                let symbol_name = &self.symbol_names[symbol];
                let inst_name = &self.inst.variant_name()[..2];
                write!(f, "{} {}, {}", inst_name, rd, symbol_name)
            }
            Sbs { rs, symbol, rt }
            | Shs { rs, symbol, rt }
            | Sws { rs, symbol, rt }
            | Sds { rs, symbol, rt } => {
                let symbol_name = &self.symbol_names[symbol];
                let inst_name = &self.inst.variant_name()[..2];
                write!(f, "{} {}, {}, {}", inst_name, rs, symbol_name, rt)
            }
            Call { func, .. } | Tail { func, .. } => {
                let symbol_name = &self.symbol_names[func];
                write!(f, "{} {}", self.inst.variant_name(), symbol_name)
            }
            LoadStack { .. }
            | SaveStack { .. }
            | StoreOutgoingArg { .. }
            | LoadIncomingArg { .. }
            | GetStackAddr { .. } => panic!("unlowered pseudo!"),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use crate::mir;

    use super::*;

    fn printer<'a>(
        inst: &'a RV64Inst,
        block_names: &'a HashMap<BlockId, String>,
        symbol_names: &'a HashMap<SymbolId, String>,
    ) -> RV64InstPrinter<'a> {
        RV64InstPrinter {
            inst,
            block_names,
            symbol_names,
        }
    }

    #[test]
    fn test_print() {
        let inst = RV64Inst::Add {
            rd: mir::Register::Physical(RV64Reg::T0),
            rs1: mir::Register::Physical(RV64Reg::S1),
            rs2: mir::Register::Physical(RV64Reg::A0),
        };
        let block_names = HashMap::new();
        let symbol_names = HashMap::new();
        assert_eq!(
            printer(&inst, &block_names, &symbol_names).to_string(),
            "add t0, s1, a0"
        );
    }

    #[test]
    fn test_print_ld() {
        let block_names = HashMap::new();
        let symbol_names = HashMap::new();

        let ld = RV64Inst::Ld {
            rd: mir::Register::Physical(RV64Reg::A0),
            rs1: mir::Register::Physical(RV64Reg::Sp),
            imm: 16,
        };
        assert_eq!(
            printer(&ld, &block_names, &symbol_names).to_string(),
            "ld a0, 16(sp)"
        );
    }
}
