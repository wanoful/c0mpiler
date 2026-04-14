pub mod rv32im;

use std::{fmt::Debug, hash::Hash};

pub trait TargetArch: Clone + 'static {
    type PhysicalReg: Clone + Copy + PartialEq + Eq + Hash + Debug;
    type MachineInst: TargetInst<PhysicalReg = Self::PhysicalReg> + Clone + Debug;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Register<R> {
    Virtual(usize),
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

pub struct MachineBlock<T: TargetArch> {
    pub id: BlockId,
    pub name: String,
    pub instructions: Vec<T::MachineInst>,

    pub predecessors: Vec<BlockId>,
    pub successors: Vec<BlockId>,
}

pub struct MachineFunction<T: TargetArch> {
    pub name: String,
    pub blocks: Vec<MachineBlock<T>>,
    pub next_vreg_id: usize,
}
