pub(crate) mod print;

use crate::{
    impossible,
    mir::{
        BlockId, FrameLayout, LoweringTarget, Register, StackSlotId, SymbolId, TargetArch,
        TargetInst, generate_reg_rewrite, rv32im::print::RV32InstPrinter,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RV32Arch;

impl TargetArch for RV32Arch {
    type PhysicalReg = RV32Reg;
    type MachineInst = RV32Inst;
    type InstPrinter<'a> = RV32InstPrinter<'a>;

    fn get_allocatable_regs() -> Vec<Self::PhysicalReg> {
        use RV32Reg::*;
        // 删除 T5, T6 以供溢出使用
        vec![
            T0, T1, T2, T3, T4, S0, S1, S2, S3, S4, S5, S6, S7, S8, S9, S10, S11, A0, A1, A2, A3,
            A4, A5, A6, A7,
        ]
    }

    fn spill_scratch_regs() -> &'static [Self::PhysicalReg]
    where
        Self: Sized,
    {
        &[RV32Reg::T5, RV32Reg::T6]
    }

    fn is_callee_saved(reg: Self::PhysicalReg) -> bool {
        matches!(
            reg,
            RV32Reg::S0
                | RV32Reg::S1
                | RV32Reg::S2
                | RV32Reg::S3
                | RV32Reg::S4
                | RV32Reg::S5
                | RV32Reg::S6
                | RV32Reg::S7
                | RV32Reg::S8
                | RV32Reg::S9
                | RV32Reg::S10
                | RV32Reg::S11
        )
    }
}

impl Default for RV32Arch {
    fn default() -> Self {
        Self
    }
}

#[rustfmt::skip]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
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

generate_reg_rewrite! {
#[rustfmt::skip]
#[derive(Debug, Clone, PartialEq, Eq)]
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
    Sbs { rs: Reg, symbol: SymbolId, rt: Reg },
    Shs { rs: Reg, symbol: SymbolId, rt: Reg },
    Sws { rs: Reg, symbol: SymbolId, rt: Reg },

    Call { func: SymbolId, num_args: usize },
    Tail { func: SymbolId, num_args: usize },

    LoadStack { rd: Reg, slot: StackSlotId },
    SaveStack { rs: Reg, slot: StackSlotId },
    StoreOutgoingArg { rs: Reg, offset: i32 },
    LoadIncomingArg { rd: Reg, offset: i32 },
    GetStackAddr { rd: Reg, slot: StackSlotId },
}
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
            | Lws { rd, .. }
            | LoadStack { rd, .. }
            | LoadIncomingArg { rd, .. }
            | GetStackAddr { rd, .. } => vec![*rd],
            Sbs { rt, .. } | Shs { rt, .. } | Sws { rt, .. } => vec![*rt],
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
            SaveStack { rs, .. } | StoreOutgoingArg { rs, .. } => {
                vec![Register::Physical(RV32Reg::Sp), *rs]
            }
            LoadStack { .. } | LoadIncomingArg { .. } | GetStackAddr { .. } => {
                vec![Register::Physical(RV32Reg::Sp)]
            }
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

    fn is_ret(&self) -> bool {
        matches!(self, RV32Inst::Ret)
    }

    fn load_imm(rd: Register<Self::PhysicalReg>, imm: i32) -> Self
    where
        Self: Sized,
    {
        RV32Inst::Li { rd, imm }
    }

    fn mv(rd: Register<Self::PhysicalReg>, rs: Register<Self::PhysicalReg>) -> Self
    where
        Self: Sized,
    {
        RV32Inst::Mv { rd, rs }
    }

    fn get_successors(&self) -> Vec<BlockId> {
        match self {
            RV32Inst::Beq { label, .. }
            | RV32Inst::Bne { label, .. }
            | RV32Inst::Blt { label, .. }
            | RV32Inst::Bge { label, .. }
            | RV32Inst::Bltu { label, .. }
            | RV32Inst::Bgeu { label, .. } => vec![*label],
            RV32Inst::Jal { label, .. } => vec![*label],
            RV32Inst::Jalr { .. } => impossible!(),
            RV32Inst::Ret => vec![],
            RV32Inst::Tail { .. } => vec![],
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
        match self {
            RV32Inst::Call { .. } => true,
            _ => false,
        }
    }
}

impl LoweringTarget for RV32Arch {
    const WORD_SIZE: usize = 4;

    fn zero_reg() -> Self::PhysicalReg {
        RV32Reg::Zero
    }

    fn return_reg() -> Self::PhysicalReg {
        RV32Reg::A0
    }

    fn ra_reg() -> Self::PhysicalReg {
        RV32Reg::Ra
    }

    fn sp_reg() -> Self::PhysicalReg {
        RV32Reg::Sp
    }

    fn arg_reg(index: usize) -> Self::PhysicalReg {
        RV32Reg::reg_a(index)
    }

    fn num_arg_regs() -> usize {
        8
    }

    fn stack_arg_size() -> usize {
        4
    }

    fn stack_arg_offset(stack_index: usize) -> i32 {
        (stack_index * 4) as i32
    }

    fn emit_add(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV32Inst::Add { rd, rs1, rs2 }
    }

    fn emit_sub(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV32Inst::Sub { rd, rs1, rs2 }
    }

    fn emit_xor(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV32Inst::Xor { rd, rs1, rs2 }
    }

    fn emit_or(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV32Inst::Or { rd, rs1, rs2 }
    }

    fn emit_and(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV32Inst::And { rd, rs1, rs2 }
    }

    fn emit_sll(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV32Inst::Sll { rd, rs1, rs2 }
    }

    fn emit_srl(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV32Inst::Srl { rd, rs1, rs2 }
    }

    fn emit_sra(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV32Inst::Sra { rd, rs1, rs2 }
    }

    fn emit_slt(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV32Inst::Slt { rd, rs1, rs2 }
    }

    fn emit_sltu(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV32Inst::Sltu { rd, rs1, rs2 }
    }

    fn emit_mul(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV32Inst::Mul { rd, rs1, rs2 }
    }

    fn emit_div(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV32Inst::Div { rd, rs1, rs2 }
    }

    fn emit_divu(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV32Inst::Divu { rd, rs1, rs2 }
    }

    fn emit_rem(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV32Inst::Rem { rd, rs1, rs2 }
    }

    fn emit_remu(rd: Reg, rs1: Reg, rs2: Reg) -> Self::MachineInst {
        RV32Inst::Remu { rd, rs1, rs2 }
    }

    fn emit_addi(rd: Reg, rs1: Reg, imm: i32) -> Self::MachineInst {
        RV32Inst::Addi { rd, rs1, imm }
    }

    fn emit_xori(rd: Reg, rs1: Reg, imm: i32) -> Self::MachineInst {
        RV32Inst::Xori { rd, rs1, imm }
    }

    fn emit_ori(rd: Reg, rs1: Reg, imm: i32) -> Self::MachineInst {
        RV32Inst::Ori { rd, rs1, imm }
    }

    fn emit_andi(rd: Reg, rs1: Reg, imm: i32) -> Self::MachineInst {
        RV32Inst::Andi { rd, rs1, imm }
    }

    fn emit_slli(rd: Reg, rs1: Reg, imm: i32) -> Self::MachineInst {
        RV32Inst::Slli { rd, rs1, imm }
    }

    fn emit_srli(rd: Reg, rs1: Reg, imm: i32) -> Self::MachineInst {
        RV32Inst::Srli { rd, rs1, imm }
    }

    fn emit_srai(rd: Reg, rs1: Reg, imm: i32) -> Self::MachineInst {
        RV32Inst::Srai { rd, rs1, imm }
    }

    fn emit_sltiu(rd: Reg, rs1: Reg, imm: i32) -> Self::MachineInst {
        RV32Inst::Sltiu { rd, rs1, imm }
    }

    fn emit_branch_ne(rs1: Reg, rs2: Reg, label: BlockId) -> Self::MachineInst {
        RV32Inst::Bne { rs1, rs2, label }
    }

    fn emit_jump(label: BlockId) -> Self::MachineInst {
        RV32Inst::Jal {
            rd: Register::Physical(RV32Reg::Zero),
            label,
        }
    }

    fn emit_call(func: SymbolId, num_args: usize) -> Self::MachineInst {
        RV32Inst::Call { func, num_args }
    }

    fn emit_ret() -> Self::MachineInst {
        RV32Inst::Ret
    }

    fn emit_load_mem(
        rd: Reg,
        rs1: Reg,
        imm: i32,
        size: usize,
        unsigned: bool,
    ) -> Self::MachineInst {
        match (size, unsigned) {
            (1, false) => RV32Inst::Lb { rd, rs1, imm },
            (1, true) => RV32Inst::Lbu { rd, rs1, imm },
            (2, false) => RV32Inst::Lh { rd, rs1, imm },
            (2, true) => RV32Inst::Lhu { rd, rs1, imm },
            (4, _) => RV32Inst::Lw { rd, rs1, imm },
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
            (1, false) => RV32Inst::Lbs { rd, symbol },
            (2, false) => RV32Inst::Lhs { rd, symbol },
            (4, _) => RV32Inst::Lws { rd, symbol },
            _ => panic!("unsupported global load kind"),
        }
    }

    fn emit_load_symbol_addr(rd: Reg, symbol: SymbolId) -> Self::MachineInst {
        RV32Inst::La { rd, label: symbol }
    }

    fn emit_store_mem(rs1: Reg, rs2: Reg, imm: i32, size: usize) -> Self::MachineInst {
        match size {
            1 => RV32Inst::Sb { rs1, rs2, imm },
            2 => RV32Inst::Sh { rs1, rs2, imm },
            4 => RV32Inst::Sw { rs1, rs2, imm },
            _ => panic!("unsupported store size"),
        }
    }

    fn emit_store_global(rs: Reg, symbol: SymbolId, size: usize, rt: Reg) -> Self::MachineInst {
        match size {
            1 => RV32Inst::Sbs { rs, symbol, rt },
            2 => RV32Inst::Shs { rs, symbol, rt },
            4 => RV32Inst::Sws { rs, symbol, rt },
            _ => panic!("unsupported global store kind"),
        }
    }

    fn emit_store_outgoing_arg(rs: Reg, offset: i32) -> Self::MachineInst {
        RV32Inst::StoreOutgoingArg { rs, offset }
    }

    fn emit_load_incoming_arg(rd: Reg, offset: i32) -> Self::MachineInst {
        RV32Inst::LoadIncomingArg { rd, offset }
    }

    fn emit_get_stack_addr(rd: Reg, slot: StackSlotId) -> Self::MachineInst {
        RV32Inst::GetStackAddr { rd, slot }
    }

    fn emit_load_stack_slot(
        rd: Register<Self::PhysicalReg>,
        slot: StackSlotId,
    ) -> Self::MachineInst {
        RV32Inst::LoadStack { rd, slot }
    }

    fn emit_store_stack_slot(
        rs: Register<Self::PhysicalReg>,
        slot: StackSlotId,
    ) -> Self::MachineInst {
        RV32Inst::SaveStack { rs, slot }
    }

    fn emit_adjust_sp(offset: isize) -> Vec<Self::MachineInst> {
        if offset == 0 {
            vec![]
        } else if -2048 <= offset && offset <= 2047 {
            vec![RV32Inst::Addi {
                rd: Register::Physical(RV32Reg::Sp),
                rs1: Register::Physical(RV32Reg::Sp),
                imm: offset as i32,
            }]
        } else {
            let temp_reg = Self::spill_scratch_regs()[0];

            vec![
                RV32Inst::Li {
                    rd: Register::Physical(temp_reg),
                    imm: offset as i32,
                },
                RV32Inst::Add {
                    rd: Register::Physical(RV32Reg::Sp),
                    rs1: Register::Physical(RV32Reg::Sp),
                    rs2: Register::Physical(temp_reg),
                },
            ]
        }
    }

    fn expand_pseudo(inst: &RV32Inst, frame_layout: &FrameLayout<RV32Arch>) -> Vec<RV32Inst>
    where
        Self: Sized,
    {
        use RV32Inst::*;
        match inst {
            LoadStack { rd, slot } => {
                let offset = frame_layout.slot_offsets[slot];
                if -2048 <= offset && offset <= 2047 {
                    vec![RV32Inst::Lw {
                        rd: *rd,
                        rs1: Register::Physical(RV32Reg::Sp),
                        imm: offset as i32,
                    }]
                } else {
                    let temp_reg = Self::spill_scratch_regs()[0];
                    vec![
                        RV32Inst::Li {
                            rd: Register::Physical(temp_reg),
                            imm: offset as i32,
                        },
                        RV32Inst::Add {
                            rd: Register::Physical(temp_reg),
                            rs1: Register::Physical(RV32Reg::Sp),
                            rs2: Register::Physical(temp_reg),
                        },
                        RV32Inst::Lw {
                            rd: *rd,
                            rs1: Register::Physical(temp_reg),
                            imm: 0,
                        },
                    ]
                }
            }
            SaveStack { rs, slot } => {
                let offset = frame_layout.slot_offsets[slot];
                if -2048 <= offset && offset <= 2047 {
                    vec![RV32Inst::Sw {
                        rs1: Register::Physical(RV32Reg::Sp),
                        rs2: *rs,
                        imm: offset as i32,
                    }]
                } else {
                    let temp_reg = Self::spill_scratch_regs()[0];
                    vec![
                        RV32Inst::Li {
                            rd: Register::Physical(temp_reg),
                            imm: offset as i32,
                        },
                        RV32Inst::Add {
                            rd: Register::Physical(temp_reg),
                            rs1: Register::Physical(RV32Reg::Sp),
                            rs2: Register::Physical(temp_reg),
                        },
                        RV32Inst::Sw {
                            rs1: Register::Physical(temp_reg),
                            rs2: *rs,
                            imm: 0,
                        },
                    ]
                }
            }
            StoreOutgoingArg { rs, offset } => {
                let offset = frame_layout.outgoing_arg_offset as i32 + *offset as i32;
                if -2048 <= offset && offset <= 2047 {
                    vec![RV32Inst::Sw {
                        rs1: Register::Physical(RV32Reg::Sp),
                        rs2: *rs,
                        imm: offset,
                    }]
                } else {
                    let temp_reg = Self::spill_scratch_regs()[0];
                    vec![
                        RV32Inst::Li {
                            rd: Register::Physical(temp_reg),
                            imm: offset,
                        },
                        RV32Inst::Add {
                            rd: Register::Physical(temp_reg),
                            rs1: Register::Physical(RV32Reg::Sp),
                            rs2: Register::Physical(temp_reg),
                        },
                        RV32Inst::Sw {
                            rs1: Register::Physical(temp_reg),
                            rs2: *rs,
                            imm: 0,
                        },
                    ]
                }
            }
            LoadIncomingArg { rd, offset } => {
                let offset = frame_layout.incoming_arg_offset as i32 + *offset as i32;
                if -2048 <= offset && offset <= 2047 {
                    vec![RV32Inst::Lw {
                        rd: *rd,
                        rs1: Register::Physical(RV32Reg::Sp),
                        imm: offset,
                    }]
                } else {
                    let temp_reg = Self::spill_scratch_regs()[0];
                    vec![
                        RV32Inst::Li {
                            rd: Register::Physical(temp_reg),
                            imm: offset,
                        },
                        RV32Inst::Add {
                            rd: Register::Physical(temp_reg),
                            rs1: Register::Physical(RV32Reg::Sp),
                            rs2: Register::Physical(temp_reg),
                        },
                        RV32Inst::Lw {
                            rd: *rd,
                            rs1: Register::Physical(temp_reg),
                            imm: 0,
                        },
                    ]
                }
            }
            GetStackAddr { rd, slot } => {
                let offset = frame_layout.slot_offsets[slot] as i32;
                if -2048 <= offset && offset <= 2047 {
                    vec![RV32Inst::Addi {
                        rd: *rd,
                        rs1: Register::Physical(RV32Reg::Sp),
                        imm: offset,
                    }]
                } else {
                    let temp_reg = Self::spill_scratch_regs()[0];
                    vec![
                        RV32Inst::Li {
                            rd: Register::Physical(temp_reg),
                            imm: offset,
                        },
                        RV32Inst::Add {
                            rd: *rd,
                            rs1: Register::Physical(RV32Reg::Sp),
                            rs2: Register::Physical(temp_reg),
                        },
                    ]
                }
            }
            _ => vec![inst.clone()],
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::mir;

    use super::*;

    #[test]
    fn test_rewrite() {
        let inst = RV32Inst::Add {
            rd: Register::Virtual(mir::VRegId(1)),
            rs1: Register::Virtual(mir::VRegId(2)),
            rs2: Register::Physical(RV32Reg::T0),
        };
        let mut use_rewrites = std::collections::HashMap::new();
        let mut def_rewrites = std::collections::HashMap::new();
        use_rewrites.insert(mir::VRegId(2), Register::Physical(RV32Reg::T1));
        def_rewrites.insert(mir::VRegId(1), Register::Physical(RV32Reg::T2));

        let rewritten = inst.rewrite_vreg(&use_rewrites, &def_rewrites);
        assert_eq!(
            rewritten,
            RV32Inst::Add {
                rd: Register::Physical(RV32Reg::T2),
                rs1: Register::Physical(RV32Reg::T1),
                rs2: Register::Physical(RV32Reg::T0),
            }
        );
    }
}
