pub mod rv32im;

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
    OutgoingArg,
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

#[derive(Default)]
pub struct MachineModule<T: TargetArch> {
    pub symbols: Vec<MachineSymbol<T>>,
    pub symbol_map: HashMap<String, SymbolId>,
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
