use slotmap::{SlotMap, new_key_type};

use crate::ir::{
    core_inst::{InstKind, OperandSlot, PhiIncoming},
    core_value::{ConstKind, GlobalKind},
    ir_type::{FunctionTypePtr, TypePtr},
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
    pub fn new() -> Self {
        Self {
            functions: SlotMap::with_key(),
            globals: SlotMap::with_key(),
            consts: SlotMap::with_key(),
        }
    }

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

    pub(crate) fn create_function(&mut self, name: String, ty: FunctionTypePtr) -> FunctionId {
        let func = self.functions.insert_with_key(|_| FunctionData {
            name,
            ty,
            args: SlotMap::with_key(),
            blocks: SlotMap::with_key(),
            insts: SlotMap::with_key(),
            block_order: Vec::new(),
            entry: BlockId::default(),
        });
        let entry = self.append_block(func, Some("entry".to_string()));
        self.func_mut(func).entry = entry.block;
        func
    }

    pub(crate) fn append_arg(
        &mut self,
        func: FunctionId,
        ty: TypePtr,
        name: Option<String>,
    ) -> ArgRef {
        let arg = self.func_mut(func).args.insert(ArgData {
            name,
            ty,
            parent: func,
            uses: Vec::new(),
        });
        ArgRef { func, arg }
    }

    pub(crate) fn new_inst(
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

    pub(crate) fn append_block(&mut self, func: FunctionId, name: Option<String>) -> BlockRef {
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

    fn assert_can_insert(&self, block: BlockRef, inst: InstRef) {
        assert_eq!(
            block.func, inst.func,
            "The instruction and the block must belong to the same function"
        );
        assert!(
            self.inst(inst).parent.is_none(),
            "The instruction already has a parent block"
        );
    }

    pub(crate) fn append_inst(&mut self, block: BlockRef, inst: InstRef) {
        self.assert_can_insert(block, inst);
        assert!(!self.inst(inst).kind.is_phi() && !self.inst(inst).kind.is_terminator());
        assert!(
            self.block(block).terminator.is_none(),
            "Cannot append instructions after a terminator"
        );
        self.block_mut(block).insts.push(inst.inst);
        self.inst_mut(inst).parent = Some(block);
        self.register_inst_use(inst);
    }

    pub(crate) fn push_front_inst(&mut self, block: BlockRef, inst: InstRef) {
        self.assert_can_insert(block, inst);
        assert!(!self.inst(inst).kind.is_phi() && !self.inst(inst).kind.is_terminator());

        self.block_mut(block).insts.insert(0, inst.inst);
        self.inst_mut(inst).parent = Some(block);
        self.register_inst_use(inst);
    }

    pub(crate) fn insert_before(&mut self, anchor: InstRef, inst: InstRef) {
        let parent = self.inst(anchor).parent.unwrap();
        self.assert_can_insert(parent, inst);
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
        self.assert_can_insert(parent, inst);
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
        self.assert_can_insert(block, inst);
        assert!(self.inst(inst).kind.is_phi());
        assert!(
            self.block(block).insts.is_empty(),
            "Phi nodes must be placed before all other instructions in the block"
        );
        assert!(
            self.block(block).terminator.is_none(),
            "Phi nodes cannot be placed in a block with a terminator"
        );
        self.block_mut(block).phis.push(inst.inst);
        self.inst_mut(inst).parent = Some(block);
        self.register_inst_use(inst);
    }

    pub(crate) fn set_terminator(&mut self, block: BlockRef, inst: InstRef) {
        self.assert_can_insert(block, inst);
        assert!(self.inst(inst).kind.is_terminator());
        assert!(
            self.block(block).terminator.is_none(),
            "A block cannot have more than one terminator"
        );
        self.block_mut(block).terminator = Some(inst.inst);
        self.inst_mut(inst).parent = Some(block);
        self.register_inst_use(inst);
    }

    pub fn append_to_block(&mut self, block: BlockRef, inst: InstRef) {
        assert!(
            self.inst(inst).parent.is_none(),
            "The instruction already has a parent block"
        );
        assert_eq!(
            block.func, inst.func,
            "The instruction and the block must belong to the same function"
        );

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

    pub(crate) fn erase_inst_from_parent(&mut self, inst: InstRef) {
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

    pub(crate) fn replace_inst_operand(
        &mut self,
        inst: InstRef,
        slot: OperandSlot,
        new_value: ValueId,
    ) {
        let old_value = self.inst_mut(inst).kind.replace_operand(slot, new_value);

        self.value_uses_mut(old_value)
            .retain(|u| !(u.user == inst && u.slot == slot));
        self.value_uses_mut(new_value)
            .push(Use { user: inst, slot });
    }

    pub(crate) fn replace_all_uses_with(&mut self, old: ValueId, new: ValueId) {
        assert_ne!(old, new, "Cannot replace a value with itself");

        let uses = self.value_uses(old).to_vec();
        for u in &uses {
            self.replace_inst_operand(u.user, u.slot, new);
        }
    }

    pub(crate) fn branch_set_then(&mut self, branch: InstRef, new_then: BlockRef) {
        assert_eq!(branch.func, new_then.func);
        match &mut self.inst_mut(branch).kind {
            InstKind::Branch { then_block, .. } => {
                *then_block = new_then.block;
            }
            _ => panic!("Expected a conditional branch instruction"),
        }
    }

    pub(crate) fn branch_set_else(&mut self, branch: InstRef, new_else: BlockRef) {
        assert_eq!(branch.func, new_else.func);
        match &mut self.inst_mut(branch).kind {
            InstKind::Branch {
                cond: Some(cond_branch),
                ..
            } => {
                cond_branch.else_block = new_else.block;
            }
            _ => panic!("Expected a conditional branch instruction"),
        }
    }

    pub(crate) fn phi_add_incoming(&mut self, phi: InstRef, block: BlockRef, value: ValueId) {
        assert_eq!(phi.func, block.func);
        match &mut self.inst_mut(phi).kind {
            InstKind::Phi { incomings } => {
                let slot = OperandSlot::PhiIncomingVal(incomings.len());
                incomings.push(PhiIncoming {
                    block: block.block,
                    value,
                });
                self.value_uses_mut(value).push(Use { user: phi, slot });
            }
            _ => panic!("Expected a phi instruction"),
        }
    }

    pub(crate) fn phi_set_incoming_value(
        &mut self,
        phi: InstRef,
        incoming_index: usize,
        new_value: ValueId,
    ) {
        match &mut self.inst_mut(phi).kind {
            InstKind::Phi { incomings } => {
                let value = incomings[incoming_index].value;
                incomings[incoming_index].value = new_value;
                self.value_uses_mut(value).retain(|u| {
                    !(u.user == phi && u.slot == OperandSlot::PhiIncomingVal(incoming_index))
                });
                self.value_uses_mut(new_value).push(Use {
                    user: phi,
                    slot: OperandSlot::PhiIncomingVal(incoming_index),
                });
            }
            _ => panic!("Expected a phi instruction"),
        }
    }

    pub(crate) fn phi_set_incoming_block(
        &mut self,
        phi: InstRef,
        incoming_index: usize,
        new_block: BlockRef,
    ) {
        assert_eq!(phi.func, new_block.func);
        match &mut self.inst_mut(phi).kind {
            InstKind::Phi { incomings } => {
                incomings[incoming_index].block = new_block.block;
            }
            _ => panic!("Expected a phi instruction"),
        }
    }

    pub(crate) fn phi_remove_incoming_from(&mut self, phi: InstRef, incoming_index: usize) {
        match &mut self.inst_mut(phi).kind {
            InstKind::Phi { incomings } => {
                let value = incomings[incoming_index].value;
                incomings.remove(incoming_index);

                let cloned = incomings.clone();

                self.value_uses_mut(value).retain(|u| {
                    !(u.user == phi && u.slot == OperandSlot::PhiIncomingVal(incoming_index))
                });

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

    pub(crate) fn successors(&self, inst: InstRef) -> Vec<BlockRef> {
        let block_ref = |block_id: BlockId| BlockRef {
            func: inst.func,
            block: block_id,
        };

        match &self.inst(inst).kind {
            InstKind::Branch {
                cond: Some(cond_branch),
                then_block,
            } => vec![block_ref(cond_branch.else_block), block_ref(*then_block)],
            InstKind::Branch {
                cond: None,
                then_block,
            } => vec![block_ref(*then_block)],
            _ => vec![],
        }
    }

    pub(crate) fn replace_successor(&mut self, inst: InstRef, old: BlockRef, new: BlockRef) {
        assert_eq!(inst.func, old.func);
        assert_eq!(inst.func, new.func);
        match &mut self.inst_mut(inst).kind {
            InstKind::Branch {
                cond: Some(cond_branch),
                then_block,
            } => {
                if cond_branch.else_block == old.block {
                    cond_branch.else_block = new.block;
                }
                if *then_block == old.block {
                    *then_block = new.block;
                }
            }
            InstKind::Branch {
                cond: None,
                then_block,
            } => {
                if *then_block == old.block {
                    *then_block = new.block;
                }
            }
            _ => {}
        }
    }
}
