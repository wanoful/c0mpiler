use slotmap::{SlotMap, new_key_type};

use crate::ir::{
    core_inst::{InstKind, OperandSlot, PhiIncoming},
    core_value::{ConstKind, GlobalKind},
    ir_type::{FunctionTypePtr, TypePtr},
    ir_value::Value,
};

new_key_type! {
    pub struct FunctionId;
    pub struct BlockId;
    pub struct InstId;
    pub struct ArgId;
    pub struct GlobalId;
    pub struct ConstId;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockRef {
    pub func: FunctionId,
    pub block: BlockId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InstRef {
    pub func: FunctionId,
    pub inst: InstId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ArgRef {
    pub func: FunctionId,
    pub arg: ArgId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueId {
    Inst(InstRef),
    Arg(ArgRef),
    Global(GlobalId),
    Const(ConstId),
}

pub struct ModuleCore {
    functions: SlotMap<FunctionId, FunctionData>,
    globals: SlotMap<GlobalId, GlobalData>,
    consts: SlotMap<ConstId, ConstData>,
}

pub struct FunctionData {
    pub name: String,
    pub ty: FunctionTypePtr,
    pub args: SlotMap<ArgId, ArgData>,
    pub blocks: SlotMap<BlockId, BlockData>,
    pub insts: SlotMap<InstId, InstData>,
    pub block_order: Vec<BlockId>,
    pub entry: BlockId,
}

pub struct BlockData {
    pub name: Option<String>,
    pub phis: Vec<InstId>,
    pub insts: Vec<InstId>,
    pub terminator: Option<InstId>,
    pub parent: FunctionId,
}

pub struct InstData {
    pub parent: Option<BlockRef>,
    pub ty: TypePtr,
    pub kind: InstKind,
    pub uses: Vec<Use>,
    pub name: Option<String>,
}

pub struct ArgData {
    pub name: Option<String>,
    pub ty: TypePtr,
    pub parent: FunctionId,
    pub uses: Vec<Use>,
}

pub struct GlobalData {
    pub name: String,
    pub ty: TypePtr,
    pub kind: GlobalKind,
    pub uses: Vec<Use>,
}

pub struct ConstData {
    pub ty: TypePtr,
    pub kind: ConstKind,
    pub uses: Vec<Use>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Use {
    pub user: InstRef,
    pub slot: OperandSlot,
}

pub enum InstPosition {
    Phi(usize),
    Inst(usize),
    Terminator,
}

impl ModuleCore {
    pub(crate) fn func(&self, f: FunctionId) -> &FunctionData {
        &self.functions[f]
    }
    pub(crate) fn func_mut(&mut self, f: FunctionId) -> &mut FunctionData {
        &mut self.functions[f]
    }

    pub(crate) fn block(&self, b: BlockRef) -> &BlockData {
        &self.functions[b.func].blocks[b.block]
    }
    pub(crate) fn block_mut(&mut self, b: BlockRef) -> &mut BlockData {
        &mut self.functions[b.func].blocks[b.block]
    }

    pub(crate) fn inst(&self, i: InstRef) -> &InstData {
        &self.functions[i.func].insts[i.inst]
    }
    pub(crate) fn inst_mut(&mut self, i: InstRef) -> &mut InstData {
        &mut self.functions[i.func].insts[i.inst]
    }

    pub(crate) fn arg(&self, a: ArgRef) -> &ArgData {
        &self.functions[a.func].args[a.arg]
    }
    pub(crate) fn arg_mut(&mut self, a: ArgRef) -> &mut ArgData {
        &mut self.functions[a.func].args[a.arg]
    }

    pub(crate) fn value_ty(&self, value_id: ValueId) -> &TypePtr {
        match value_id {
            ValueId::Inst(inst_ref) => &self.inst(inst_ref).ty,
            ValueId::Arg(arg_ref) => &self.arg(arg_ref).ty,
            ValueId::Global(global_id) => &self.globals[global_id].ty,
            ValueId::Const(const_id) => &self.consts[const_id].ty,
        }
    }

    pub(crate) fn value_uses(&self, value_id: ValueId) -> &[Use] {
        match value_id {
            ValueId::Inst(inst_ref) => &self.inst(inst_ref).uses,
            ValueId::Arg(arg_ref) => &self.arg(arg_ref).uses,
            ValueId::Global(global_id) => &self.globals[global_id].uses,
            ValueId::Const(const_id) => &self.consts[const_id].uses,
        }
    }

    pub(crate) fn value_uses_mut(&mut self, value_id: ValueId) -> &mut Vec<Use> {
        match value_id {
            ValueId::Inst(inst_ref) => &mut self.inst_mut(inst_ref).uses,
            ValueId::Arg(arg_ref) => &mut self.arg_mut(arg_ref).uses,
            ValueId::Global(global_id) => &mut self.globals[global_id].uses,
            ValueId::Const(const_id) => &mut self.consts[const_id].uses,
        }
    }

    fn locate_inst(&self, inst_ref: InstRef) -> InstPosition {
        let parent = self.inst(inst_ref).parent.unwrap();
        let block_data = self.block(parent);
        if let Some(pos) = block_data.insts.iter().position(|&i| i == inst_ref.inst) {
            InstPosition::Inst(pos)
        } else if let Some(pos) = block_data.phis.iter().position(|&i| i == inst_ref.inst) {
            InstPosition::Phi(pos)
        } else if block_data.terminator == Some(inst_ref.inst) {
            InstPosition::Terminator
        } else {
            panic!(
                "Instruction {:#?} is not found in its parent block {:#?}",
                inst_ref, parent
            );
        }
    }

    fn new_inst(
        &mut self,
        func: FunctionId,
        ty: TypePtr,
        kind: InstKind,
        name: Option<String>,
    ) -> InstRef {
        let inst_id = self.func_mut(func).insts.insert(InstData {
            parent: None,
            ty,
            kind,
            uses: Vec::new(),
            name,
        });
        InstRef {
            func,
            inst: inst_id,
        }
    }

    fn append_block(&mut self, func: FunctionId, name: Option<String>) -> BlockRef {
        let block_id = self.func_mut(func).blocks.insert(BlockData {
            name,
            phis: Vec::new(),
            insts: Vec::new(),
            terminator: None,
            parent: func,
        });
        let block_ref = BlockRef {
            func,
            block: block_id,
        };
        self.func_mut(func).block_order.push(block_id);
        block_ref
    }

    pub(crate) fn append_inst(&mut self, block: BlockRef, inst: InstRef) {
        self.block_mut(block).insts.push(inst.inst);
        self.inst_mut(inst).parent = Some(block);
        self.register_inst_use(inst);
    }

    pub(crate) fn push_front_inst(&mut self, block: BlockRef, inst: InstRef) {
        self.block_mut(block).insts.insert(0, inst.inst);
        self.inst_mut(inst).parent = Some(block);
        self.register_inst_use(inst);
    }

    pub(crate) fn insert_before(&mut self, anchor: InstRef, inst: InstRef) {
        let parent = self.inst(anchor).parent.unwrap();
        match self.locate_inst(anchor) {
            InstPosition::Phi(i) => {
                debug_assert!(self.inst(inst).kind.is_phi());
                self.block_mut(parent).phis.insert(i, inst.inst);
            }
            InstPosition::Inst(i) => {
                debug_assert!(
                    !(self.inst(inst).kind.is_phi() || self.inst(inst).kind.is_terminator())
                );
                self.block_mut(parent).insts.insert(i, inst.inst);
            }
            InstPosition::Terminator => {
                debug_assert!(
                    !(self.inst(inst).kind.is_phi() || self.inst(inst).kind.is_terminator())
                );
                self.block_mut(parent).insts.push(inst.inst);
            }
        }
        self.inst_mut(inst).parent = Some(parent);

        self.register_inst_use(inst);
    }

    pub(crate) fn insert_after(&mut self, anchor: InstRef, inst: InstRef) {
        let parent = self.inst(anchor).parent.unwrap();
        match self.locate_inst(anchor) {
            InstPosition::Phi(i) => {
                debug_assert!(self.inst(inst).kind.is_phi());
                self.block_mut(parent).phis.insert(i + 1, inst.inst);
            }
            InstPosition::Inst(i) => {
                debug_assert!(
                    !(self.inst(inst).kind.is_phi() || self.inst(inst).kind.is_terminator())
                );
                self.block_mut(parent).insts.insert(i + 1, inst.inst);
            }
            InstPosition::Terminator => {
                panic!("Cannot insert after a terminator instruction");
            }
        }
        self.inst_mut(inst).parent = Some(parent);

        self.register_inst_use(inst);
    }

    pub(crate) fn append_phi(&mut self, block: BlockRef, inst: InstRef) {
        self.block_mut(block).phis.push(inst.inst);
        self.inst_mut(inst).parent = Some(block);
        self.register_inst_use(inst);
    }

    pub(crate) fn set_terminator(&mut self, block: BlockRef, inst: InstRef) {
        self.block_mut(block).terminator = Some(inst.inst);
        self.inst_mut(inst).parent = Some(block);
        self.register_inst_use(inst);
    }

    pub fn append_to_block(&mut self, block: BlockRef, inst: InstRef) {
        let kind = &self.inst(inst).kind;

        if kind.is_phi() {
            assert!(
                self.block(block).insts.is_empty(),
                "Phi nodes must be placed before all other instructions in the block"
            );
            assert!(
                self.block(block).terminator.is_none(),
                "Phi nodes cannot be placed in a block with a terminator"
            );
            self.append_phi(block, inst);
        } else if kind.is_terminator() {
            assert!(
                self.block(block).terminator.is_none(),
                "A block cannot have more than one terminator"
            );
            self.set_terminator(block, inst);
        } else {
            assert!(
                self.block(block).terminator.is_none(),
                "Cannot append instructions after a terminator"
            );
            self.append_inst(block, inst);
        }
    }

    fn detach_inst(&mut self, inst: InstRef) {
        let parent = self.inst(inst).parent.unwrap();

        match self.locate_inst(inst) {
            InstPosition::Inst(pos) => {
                self.block_mut(parent).insts.remove(pos);
            }
            InstPosition::Phi(pos) => {
                self.block_mut(parent).phis.remove(pos);
            }
            InstPosition::Terminator => self.block_mut(parent).terminator = None,
        }

        self.inst_mut(inst).parent = None;
    }

    fn erase_inst_from_parent(&mut self, inst: InstRef) {
        let value = ValueId::Inst(inst);
        assert!(
            self.value_uses(value).is_empty(),
            "Cannot erase the instruction because it is still used",
        );

        self.unregister_inst_use(inst);

        self.detach_inst(inst);

        self.func_mut(inst.func).insts.remove(inst.inst);
    }

    fn register_inst_use(&mut self, user: InstRef) {
        let kind = self.inst(user).kind.clone();
        kind.for_each_value_operand(|value, slot| {
            self.value_uses_mut(value).push(Use { user, slot });
        });
    }

    fn unregister_inst_use(&mut self, user: InstRef) {
        let kind = self.inst(user).kind.clone();
        kind.for_each_value_operand(|value, slot| {
            let uses = self.value_uses_mut(value);
            if let Some(pos) = uses.iter().position(|u| u.user == user && u.slot == slot) {
                uses.remove(pos);
            }
        });
    }

    fn replace_inst_operand(&mut self, inst: InstRef, slot: OperandSlot, new_value: ValueId) {
        self.inst_mut(inst).kind.replace_operand(slot, new_value);
    }

    fn replace_all_uses_with(&mut self, old: ValueId, new: ValueId) {
        assert_ne!(old, new, "Cannot replace a value with itself");

        let uses = self.value_uses(old).to_vec();
        for u in &uses {
            self.inst_mut(u.user).kind.replace_operand(u.slot, new);
            self.value_uses_mut(new).push(*u);
        }
        self.value_uses_mut(old).clear();
    }

    fn branch_set_then(&mut self, branch: InstRef, new_then: BlockId) {
        match &mut self.inst_mut(branch).kind {
            InstKind::Branch {
                cond: Some(cond_branch),
                ..
            } => {
                cond_branch.else_block = new_then;
            }
            _ => panic!("Expected a conditional branch instruction"),
        }
    }

    fn branch_set_else(&mut self, branch: InstRef, new_else: BlockId) {
        match &mut self.inst_mut(branch).kind {
            InstKind::Branch {
                cond: Some(cond_branch),
                ..
            } => {
                cond_branch.else_block = new_else;
            }
            _ => panic!("Expected a conditional branch instruction"),
        }
    }

    fn phi_add_incoming(&mut self, phi: InstRef, block: BlockId, value: ValueId) {
        match &mut self.inst_mut(phi).kind {
            InstKind::Phi { incomings } => {
                incomings.push(PhiIncoming { block, value });
            }
            _ => panic!("Expected a phi instruction"),
        }
    }

    fn phi_set_incoming_value(&mut self, phi: InstRef, incoming_index: usize, new_value: ValueId) {
        match &mut self.inst_mut(phi).kind {
            InstKind::Phi { incomings } => {
                incomings[incoming_index].value = new_value;
            }
            _ => panic!("Expected a phi instruction"),
        }
    }

    fn phi_set_incoming_block(&mut self, phi: InstRef, incoming_index: usize, new_block: BlockId) {
        match &mut self.inst_mut(phi).kind {
            InstKind::Phi { incomings } => {
                incomings[incoming_index].block = new_block;
            }
            _ => panic!("Expected a phi instruction"),
        }
    }

    fn phi_remove_incoming_from(&mut self, phi: InstRef, incoming_index: usize) {
        match &mut self.inst_mut(phi).kind {
            InstKind::Phi { incomings } => {
                incomings.remove(incoming_index);

                let cloned = incomings.clone();

                for i in incoming_index..cloned.len() {
                    let id = cloned[i].value;
                    self.value_uses_mut(id)
                        .iter_mut()
                        .find(|u| u.user == phi && u.slot == OperandSlot::PhiIncomingVal(i + 1))
                        .unwrap()
                        .slot = OperandSlot::PhiIncomingVal(i);
                }
            }
            _ => panic!("Expected a phi instruction"),
        }
    }

    fn successors(&self, inst: InstRef) -> Vec<BlockId> {
        match &self.inst(inst).kind {
            InstKind::Branch {
                cond: Some(cond_branch),
                then_block,
            } => vec![cond_branch.else_block, *then_block],
            InstKind::Branch {
                cond: None,
                then_block,
            } => vec![*then_block],
            _ => vec![],
        }
    }

    fn replace_successor(&mut self, inst: InstRef, old: BlockId, new: BlockId) {
        match &mut self.inst_mut(inst).kind {
            InstKind::Branch {
                cond: Some(cond_branch),
                then_block,
            } => {
                if cond_branch.else_block == old {
                    cond_branch.else_block = new;
                }
                if *then_block == old {
                    *then_block = new;
                }
            }
            InstKind::Branch {
                cond: None,
                then_block,
            } => {
                if *then_block == old {
                    *then_block = new;
                }
            }
            _ => {}
        }
    }
}
