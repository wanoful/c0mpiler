pub mod rv32im;
pub mod lower;

use std::{collections::HashMap, fmt::Debug, hash::Hash};

pub trait TargetArch: Clone + 'static {
    type PhysicalReg: Clone + Copy + PartialEq + Eq + Hash + Debug;
    type MachineInst: TargetInst<PhysicalReg = Self::PhysicalReg> + Clone + Debug;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VRegId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SymbolId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Register<R> {
    Virtual(VRegId),
    Physical(R),
}

pub trait TargetInst {
    type PhysicalReg;

    fn def_regs(&self) -> Vec<Register<Self::PhysicalReg>>;
    fn use_regs(&self) -> Vec<Register<Self::PhysicalReg>>;
    fn is_terminator(&self) -> bool;

    fn load_imm(rd: Register<Self::PhysicalReg>, imm: i32) -> Self
    where
        Self: Sized;

    fn mv(rd: Register<Self::PhysicalReg>, rs: Register<Self::PhysicalReg>) -> Self
    where
        Self: Sized;    
}

pub trait LoweringTarget: TargetArch + Default {
    fn zero_reg() -> Self::PhysicalReg;
    fn return_reg() -> Self::PhysicalReg;
    fn arg_reg(index: usize) -> Self::PhysicalReg;
    fn num_arg_regs() -> usize;
    fn stack_arg_size() -> usize;
    fn stack_arg_offset(stack_index: usize) -> i32;

    fn emit_add(
        rd: Register<Self::PhysicalReg>,
        rs1: Register<Self::PhysicalReg>,
        rs2: Register<Self::PhysicalReg>,
    ) -> Self::MachineInst;
    fn emit_sub(
        rd: Register<Self::PhysicalReg>,
        rs1: Register<Self::PhysicalReg>,
        rs2: Register<Self::PhysicalReg>,
    ) -> Self::MachineInst;
    fn emit_xor(
        rd: Register<Self::PhysicalReg>,
        rs1: Register<Self::PhysicalReg>,
        rs2: Register<Self::PhysicalReg>,
    ) -> Self::MachineInst;
    fn emit_or(
        rd: Register<Self::PhysicalReg>,
        rs1: Register<Self::PhysicalReg>,
        rs2: Register<Self::PhysicalReg>,
    ) -> Self::MachineInst;
    fn emit_and(
        rd: Register<Self::PhysicalReg>,
        rs1: Register<Self::PhysicalReg>,
        rs2: Register<Self::PhysicalReg>,
    ) -> Self::MachineInst;
    fn emit_sll(
        rd: Register<Self::PhysicalReg>,
        rs1: Register<Self::PhysicalReg>,
        rs2: Register<Self::PhysicalReg>,
    ) -> Self::MachineInst;
    fn emit_srl(
        rd: Register<Self::PhysicalReg>,
        rs1: Register<Self::PhysicalReg>,
        rs2: Register<Self::PhysicalReg>,
    ) -> Self::MachineInst;
    fn emit_sra(
        rd: Register<Self::PhysicalReg>,
        rs1: Register<Self::PhysicalReg>,
        rs2: Register<Self::PhysicalReg>,
    ) -> Self::MachineInst;
    fn emit_slt(
        rd: Register<Self::PhysicalReg>,
        rs1: Register<Self::PhysicalReg>,
        rs2: Register<Self::PhysicalReg>,
    ) -> Self::MachineInst;
    fn emit_sltu(
        rd: Register<Self::PhysicalReg>,
        rs1: Register<Self::PhysicalReg>,
        rs2: Register<Self::PhysicalReg>,
    ) -> Self::MachineInst;
    fn emit_mul(
        rd: Register<Self::PhysicalReg>,
        rs1: Register<Self::PhysicalReg>,
        rs2: Register<Self::PhysicalReg>,
    ) -> Self::MachineInst;
    fn emit_div(
        rd: Register<Self::PhysicalReg>,
        rs1: Register<Self::PhysicalReg>,
        rs2: Register<Self::PhysicalReg>,
    ) -> Self::MachineInst;
    fn emit_divu(
        rd: Register<Self::PhysicalReg>,
        rs1: Register<Self::PhysicalReg>,
        rs2: Register<Self::PhysicalReg>,
    ) -> Self::MachineInst;
    fn emit_rem(
        rd: Register<Self::PhysicalReg>,
        rs1: Register<Self::PhysicalReg>,
        rs2: Register<Self::PhysicalReg>,
    ) -> Self::MachineInst;
    fn emit_remu(
        rd: Register<Self::PhysicalReg>,
        rs1: Register<Self::PhysicalReg>,
        rs2: Register<Self::PhysicalReg>,
    ) -> Self::MachineInst;

    fn emit_addi(
        rd: Register<Self::PhysicalReg>,
        rs1: Register<Self::PhysicalReg>,
        imm: i32,
    ) -> Self::MachineInst;
    fn emit_xori(
        rd: Register<Self::PhysicalReg>,
        rs1: Register<Self::PhysicalReg>,
        imm: i32,
    ) -> Self::MachineInst;
    fn emit_ori(
        rd: Register<Self::PhysicalReg>,
        rs1: Register<Self::PhysicalReg>,
        imm: i32,
    ) -> Self::MachineInst;
    fn emit_andi(
        rd: Register<Self::PhysicalReg>,
        rs1: Register<Self::PhysicalReg>,
        imm: i32,
    ) -> Self::MachineInst;
    fn emit_slli(
        rd: Register<Self::PhysicalReg>,
        rs1: Register<Self::PhysicalReg>,
        imm: i32,
    ) -> Self::MachineInst;
    fn emit_srli(
        rd: Register<Self::PhysicalReg>,
        rs1: Register<Self::PhysicalReg>,
        imm: i32,
    ) -> Self::MachineInst;
    fn emit_srai(
        rd: Register<Self::PhysicalReg>,
        rs1: Register<Self::PhysicalReg>,
        imm: i32,
    ) -> Self::MachineInst;
    fn emit_sltiu(
        rd: Register<Self::PhysicalReg>,
        rs1: Register<Self::PhysicalReg>,
        imm: i32,
    ) -> Self::MachineInst;

    fn emit_branch_ne(
        rs1: Register<Self::PhysicalReg>,
        rs2: Register<Self::PhysicalReg>,
        label: BlockId,
    ) -> Self::MachineInst;
    fn emit_jump(label: BlockId) -> Self::MachineInst;
    fn emit_call(func: SymbolId, num_args: usize) -> Self::MachineInst;
    fn emit_ret() -> Self::MachineInst;

    fn emit_load_mem(
        rd: Register<Self::PhysicalReg>,
        rs1: Register<Self::PhysicalReg>,
        imm: i32,
        size: usize,
        unsigned: bool,
    ) -> Self::MachineInst;
    fn emit_load_global(
        rd: Register<Self::PhysicalReg>,
        symbol: SymbolId,
        size: usize,
        unsigned: bool,
    ) -> Self::MachineInst;
    fn emit_store_mem(
        rs1: Register<Self::PhysicalReg>,
        rs2: Register<Self::PhysicalReg>,
        imm: i32,
        size: usize,
    ) -> Self::MachineInst;
    fn emit_store_global(
        rs: Register<Self::PhysicalReg>,
        symbol: SymbolId,
        size: usize,
    ) -> Self::MachineInst;

    fn emit_store_outgoing_arg(
        rs: Register<Self::PhysicalReg>,
        offset: i32,
    ) -> Self::MachineInst;
    fn emit_load_incoming_arg(
        rd: Register<Self::PhysicalReg>,
        offset: i32,
    ) -> Self::MachineInst;
    fn emit_get_stack_addr(
        rd: Register<Self::PhysicalReg>,
        slot: StackSlotId,
    ) -> Self::MachineInst;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StackSlotId(pub usize);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StackSlotKind {
    Alloca,
    Spill,
    CalleeSaved,
    LocalTemp,
}

#[derive(Debug, Clone)]
pub struct StackSlot {
    pub id: StackSlotId,
    pub size: usize,
    pub align: usize,
    pub kind: StackSlotKind,
}

pub struct MachineBlock<T: TargetArch> {
    pub id: BlockId,
    pub name: String,
    pub instructions: Vec<T::MachineInst>,
}

pub struct FrameInfo {
    pub stack_slots: Vec<StackSlot>,
    pub max_align: usize,
    pub max_outgoing_arg_size: usize,
}

pub struct FrameLayout {
    pub frame_size: usize,
    pub slot_offsets: HashMap<StackSlotId, isize>,
    pub outgoing_arg_offset: isize,
    pub incoming_arg_offset: isize,
}

pub struct MachineFunction<T: TargetArch> {
    pub name: String,
    pub blocks: Vec<MachineBlock<T>>,
    pub next_vreg_id: usize,
    pub entry: BlockId,
    pub frame_info: FrameInfo,
    pub frame_layout: FrameLayout,
}

impl<T: TargetArch> MachineFunction<T> {
    pub fn new_stack_slot(
        &mut self,
        size: usize,
        align: usize,
        kind: StackSlotKind,
    ) -> StackSlotId {
        self.frame_info.max_align = self.frame_info.max_align.max(align);

        let id = StackSlotId(self.frame_info.stack_slots.len());
        self.frame_info.stack_slots.push(StackSlot {
            id,
            size,
            align,
            kind,
        });
        id
    }

    pub fn new_vreg(&mut self) -> VRegId {
        let id = VRegId(self.next_vreg_id);
        self.next_vreg_id += 1;
        id
    }

    pub fn record_outgoing_arg(&mut self, size: usize) {
        self.frame_info.max_outgoing_arg_size = self.frame_info.max_outgoing_arg_size.max(size);
    }

    pub fn get_block_mut(&mut self, block_id: BlockId) -> Option<&mut MachineBlock<T>> {
        self.blocks.iter_mut().find(|b| b.id == block_id)
    }

    pub fn get_block(&self, block_id: BlockId) -> Option<&MachineBlock<T>> {
        self.blocks.iter().find(|b| b.id == block_id)
    }
}

pub enum Linkage {
    External,
    Internal,
}

pub enum MachineSymbolKind<T: TargetArch> {
    Function(MachineFunction<T>),
    ExternalPlaceholder,
    Data(Vec<u8>),
    Bss { size: usize },
}

pub enum MachineSegment {
    Text,
    Data,
    ReadOnlyData,
    Bss,
}

pub struct MachineSymbol<T: TargetArch> {
    pub id: SymbolId,
    pub name: String,
    pub kind: MachineSymbolKind<T>,
    pub segment: MachineSegment,

    pub linkage: Linkage,
    pub alignment: usize,
}

pub struct MachineModule<T: TargetArch> {
    pub symbols: Vec<MachineSymbol<T>>,
    pub symbol_map: HashMap<String, SymbolId>,
}

impl<T: TargetArch> Default for MachineModule<T> {
    fn default() -> Self {
        Self {
            symbols: Vec::new(),
            symbol_map: HashMap::new(),
        }
    }
}

impl<T: TargetArch> MachineModule<T> {
    pub fn new_symbol(
        &mut self,
        name: String,
        kind: MachineSymbolKind<T>,
        segment: MachineSegment,
        linkage: Linkage,
        alignment: usize,
    ) -> SymbolId {
        let id = SymbolId(self.symbols.len());
        let replaced = self.symbol_map.insert(name.clone(), id);
        debug_assert!(replaced.is_none(), "Symbol name '{}' already exists", name);
        self.symbols.push(MachineSymbol {
            id,
            name,
            kind,
            segment,
            linkage,
            alignment,
        });
        id
    }
}
