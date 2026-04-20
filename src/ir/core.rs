use std::{collections::HashMap, rc::Rc};

use slotmap::{SlotMap, new_key_type};

use crate::ir::{
    attribute::AttributeDiscriminants,
    core_inst::{InstKind, OperandSlot, PhiIncoming},
    core_value::{ConstKind, GlobalKind},
    ir_type::{FunctionTypePtr, TypePtr},
    ir_value::{
        Constant, ConstantPtr, ICmpCode as LegacyICmpCode,
        InstructionKind as LegacyInstructionKind, InstructionPtr, Value, ValuePtr,
    },
    ir_value::BinaryOpcode as LegacyBinaryOpcode,
    layout::TargetDataLayout,
    LLVMModule,
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
    pub(crate) named_structs: HashMap<String, TypePtr>,
    pub(crate) target_data_layout: Option<TargetDataLayout>,
}

pub struct FunctionData {
    pub name: String,
    pub ty: FunctionTypePtr,
    pub args: SlotMap<ArgId, ArgData>,
    pub blocks: SlotMap<BlockId, BlockData>,
    pub insts: SlotMap<InstId, InstData>,
    pub block_order: Vec<BlockId>,
    pub entry: BlockId,
    pub sret: Option<TypePtr>,
    pub is_declare: bool,
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
            named_structs: HashMap::new(),
            target_data_layout: None,
        }
    }

    pub(crate) fn func(&self, f: FunctionId) -> &FunctionData {
        &self.functions[f]
    }
    pub(crate) fn func_mut(&mut self, f: FunctionId) -> &mut FunctionData {
        &mut self.functions[f]
    }

    pub(crate) fn functions_in_order(&self) -> Vec<FunctionId> {
        let mut functions = self.functions.keys().collect::<Vec<_>>();
        functions.sort_by(|lhs, rhs| self.func(*lhs).name.cmp(&self.func(*rhs).name));
        functions
    }

    pub(crate) fn globals_in_order(&self) -> Vec<GlobalId> {
        let mut globals = self.globals.keys().collect::<Vec<_>>();
        globals.sort_by(|lhs, rhs| self.global(*lhs).name.cmp(&self.global(*rhs).name));
        globals
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

    pub(crate) fn global(&self, g: GlobalId) -> &GlobalData {
        &self.globals[g]
    }

    pub(crate) fn const_data(&self, c: ConstId) -> &ConstData {
        &self.consts[c]
    }

    pub(crate) fn args_in_order(&self, func: FunctionId) -> Vec<ArgRef> {
        self.func(func)
            .args
            .keys()
            .map(|arg| ArgRef { func, arg })
            .collect()
    }

    pub(crate) fn blocks_in_order(&self, func: FunctionId) -> Vec<BlockRef> {
        self.func(func)
            .block_order
            .iter()
            .copied()
            .map(|block| BlockRef { func, block })
            .collect()
    }

    pub(crate) fn phis_in_order(&self, block: BlockRef) -> Vec<InstRef> {
        self.block(block)
            .phis
            .iter()
            .copied()
            .map(|inst| InstRef {
                func: block.func,
                inst,
            })
            .collect()
    }

    pub(crate) fn insts_in_order(&self, block: BlockRef) -> Vec<InstRef> {
        self.block(block)
            .insts
            .iter()
            .copied()
            .map(|inst| InstRef {
                func: block.func,
                inst,
            })
            .collect()
    }

    pub(crate) fn terminator(&self, block: BlockRef) -> Option<InstRef> {
        self.block(block).terminator.map(|inst| InstRef {
            func: block.func,
            inst,
        })
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

    pub(crate) fn define_function(&mut self, name: String, ty: FunctionTypePtr) -> FunctionId {
        self.functions.insert_with_key(|_| FunctionData {
            name,
            ty,
            args: SlotMap::with_key(),
            blocks: SlotMap::with_key(),
            insts: SlotMap::with_key(),
            block_order: Vec::new(),
            entry: BlockId::default(),
            sret: None,
            is_declare: false,
        })
    }

    pub(crate) fn create_function(&mut self, name: String, ty: FunctionTypePtr) -> FunctionId {
        let func = self.define_function(name, ty);
        let entry = self.append_block(func, Some("entry".to_string()));
        self.func_mut(func).entry = entry.block;
        func
    }

    pub(crate) fn declare_function(&mut self, name: String, ty: FunctionTypePtr) -> FunctionId {
        self.functions.insert_with_key(|_| FunctionData {
            name,
            ty,
            args: SlotMap::with_key(),
            blocks: SlotMap::with_key(),
            insts: SlotMap::with_key(),
            block_order: Vec::new(),
            entry: BlockId::default(),
            sret: None,
            is_declare: true,
        })
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

    pub(crate) fn set_sret(&mut self, func: FunctionId, ty: TypePtr) {
        self.func_mut(func).sret = Some(ty);
    }

    pub(crate) fn add_global(
        &mut self,
        name: String,
        ty: TypePtr,
        kind: GlobalKind,
    ) -> GlobalId {
        self.globals.insert(GlobalData {
            name,
            ty,
            kind,
            uses: Vec::new(),
        })
    }

    pub(crate) fn add_const(&mut self, ty: TypePtr, kind: ConstKind) -> ConstId {
        self.consts.insert(ConstData {
            ty,
            kind,
            uses: Vec::new(),
        })
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

    pub fn from_legacy_module(module: &LLVMModule) -> Self {
        let mut core = ModuleCore::new();
        {
            let ctx = module.ctx_impl.borrow();
            core.target_data_layout = Some(ctx.type_layout_engine.target());
            core.named_structs = ctx.named_strcut_ty.clone();
        }

        let mut const_map: HashMap<*const Value, ConstId> = HashMap::new();
        let mut global_map: HashMap<*const Value, GlobalId> = HashMap::new();
        let mut function_map: HashMap<*const Value, FunctionId> = HashMap::new();
        let mut arg_map: HashMap<*const Value, ArgRef> = HashMap::new();
        let mut block_map: HashMap<*const Value, BlockRef> = HashMap::new();
        let mut inst_map: HashMap<*const Value, InstRef> = HashMap::new();

        for func in module.functions_in_order() {
            let name = func.get_name().unwrap();
            let func_ty = FunctionTypePtr(func.as_global_object().get_inner_ty().clone());
            let is_declare = func.as_function().blocks.borrow().is_empty();
            let new_func = if is_declare {
                core.declare_function(name.clone(), func_ty)
            } else {
                core.define_function(name.clone(), func_ty)
            };

            if let Some(attr) = func
                .as_function()
                .get_param_attr(0, AttributeDiscriminants::StructReturn)
            {
                core.set_sret(new_func, attr.into_struct_return().unwrap());
            }

            let func_value: ValuePtr = func.clone().into();
            function_map.insert(Rc::as_ptr(&func_value), new_func);
            let global = core.add_global(
                name,
                func_value.get_type().clone(),
                GlobalKind::Function(new_func),
            );
            global_map.insert(Rc::as_ptr(&func_value), global);

            for arg in func.as_function().args() {
                let arg_ref = core.append_arg(new_func, arg.get_type().clone(), arg.get_name());
                let arg_value: ValuePtr = arg.clone().into();
                arg_map.insert(Rc::as_ptr(&arg_value), arg_ref);
            }

            if !is_declare {
                for (index, block) in func.as_function().blocks.borrow().iter().enumerate() {
                    let block_ref = core.append_block(new_func, block.get_name());
                    if index == 0 {
                        core.func_mut(new_func).entry = block_ref.block;
                    }
                    let block_value: ValuePtr = block.clone().into();
                    block_map.insert(Rc::as_ptr(&block_value), block_ref);
                }
            }
        }

        for global in module.global_variables() {
            let global_value: ValuePtr = global.clone().into();
            let initializer = core.convert_legacy_const(
                global.as_global_variable().initializer.clone(),
                &mut const_map,
            );
            let new_global = core.add_global(
                global.get_name().unwrap(),
                global_value.get_type().clone(),
                GlobalKind::GlobalVariable {
                    is_constant: global.as_global_variable().is_constant,
                    initializer: Some(initializer),
                },
            );
            global_map.insert(Rc::as_ptr(&global_value), new_global);
        }

        for func in module.functions_in_order() {
            if func.as_function().blocks.borrow().is_empty() {
                continue;
            }
            let func_value: ValuePtr = func.clone().into();
            let new_func = function_map[&Rc::as_ptr(&func_value)];

            for block in func.as_function().blocks.borrow().iter() {
                let instructions = block.as_basic_block().instructions.borrow();
                for inst in instructions.iter() {
                    let new_inst = core.new_inst(
                        new_func,
                        inst.get_type().clone(),
                        InstKind::Unreachable,
                        inst.get_name(),
                    );
                    let inst_value: ValuePtr = inst.clone().into();
                    inst_map.insert(Rc::as_ptr(&inst_value), new_inst);
                }
            }
        }

        for func in module.functions_in_order() {
            if func.as_function().blocks.borrow().is_empty() {
                continue;
            }

            for block in func.as_function().blocks.borrow().iter() {
                let block_value: ValuePtr = block.clone().into();
                let new_block = block_map[&Rc::as_ptr(&block_value)];
                let instructions = block.as_basic_block().instructions.borrow();
                for inst in instructions.iter() {
                    let inst_value: ValuePtr = inst.clone().into();
                    let new_inst = inst_map[&Rc::as_ptr(&inst_value)];
                    let kind = core.convert_legacy_inst_kind(
                        inst,
                        &mut const_map,
                        &global_map,
                        &arg_map,
                        &block_map,
                        &inst_map,
                        &function_map,
                    );
                    core.inst_mut(new_inst).kind = kind;
                    core.append_to_block(new_block, new_inst);
                }
            }
        }

        core
    }

    fn convert_legacy_const(
        &mut self,
        constant: ConstantPtr,
        const_map: &mut HashMap<*const Value, ConstId>,
    ) -> ConstId {
        let value: ValuePtr = constant.clone().into();
        let raw = Rc::as_ptr(&value);
        if let Some(id) = const_map.get(&raw) {
            return *id;
        }

        let kind = match constant.as_constant() {
            Constant::ConstantInt(value) => ConstKind::Int(value.0 as i64),
            Constant::ConstantArray(values) => ConstKind::Array(
                values
                    .0
                    .iter()
                    .cloned()
                    .map(|value| self.convert_legacy_const(value, const_map))
                    .collect(),
            ),
            Constant::ConstantStruct(values) => ConstKind::Struct(
                values
                    .0
                    .iter()
                    .cloned()
                    .map(|value| self.convert_legacy_const(value, const_map))
                    .collect(),
            ),
            Constant::ConstantString(value) => ConstKind::String(value.0.clone()),
            Constant::ConstantNull(_) => ConstKind::Null,
        };
        let id = self.add_const(constant.get_type().clone(), kind);
        const_map.insert(raw, id);
        id
    }

    fn convert_legacy_value(
        &mut self,
        value: &ValuePtr,
        const_map: &mut HashMap<*const Value, ConstId>,
        global_map: &HashMap<*const Value, GlobalId>,
        arg_map: &HashMap<*const Value, ArgRef>,
        inst_map: &HashMap<*const Value, InstRef>,
    ) -> ValueId {
        let raw = Rc::as_ptr(value);
        if let Some(global) = global_map.get(&raw) {
            return ValueId::Global(*global);
        }
        if let Some(arg) = arg_map.get(&raw) {
            return ValueId::Arg(*arg);
        }
        if let Some(inst) = inst_map.get(&raw) {
            return ValueId::Inst(*inst);
        }
        if value.kind.as_constant().is_some() {
            return ValueId::Const(self.convert_legacy_const(
                ConstantPtr(value.clone()),
                const_map,
            ));
        }
        panic!("unsupported legacy value in conversion: {:?}", value);
    }

    fn convert_legacy_inst_kind(
        &mut self,
        inst: &InstructionPtr,
        const_map: &mut HashMap<*const Value, ConstId>,
        global_map: &HashMap<*const Value, GlobalId>,
        arg_map: &HashMap<*const Value, ArgRef>,
        block_map: &HashMap<*const Value, BlockRef>,
        inst_map: &HashMap<*const Value, InstRef>,
        function_map: &HashMap<*const Value, FunctionId>,
    ) -> InstKind {
        let operands = &inst.as_instruction().operands;
        match &inst.as_instruction().kind {
            LegacyInstructionKind::Binary(op) => InstKind::Binary {
                op: match op {
                    LegacyBinaryOpcode::Add => crate::ir::core_inst::BinaryOpcode::Add,
                    LegacyBinaryOpcode::Sub => crate::ir::core_inst::BinaryOpcode::Sub,
                    LegacyBinaryOpcode::Mul => crate::ir::core_inst::BinaryOpcode::Mul,
                    LegacyBinaryOpcode::UDiv => crate::ir::core_inst::BinaryOpcode::UDiv,
                    LegacyBinaryOpcode::SDiv => crate::ir::core_inst::BinaryOpcode::SDiv,
                    LegacyBinaryOpcode::URem => crate::ir::core_inst::BinaryOpcode::URem,
                    LegacyBinaryOpcode::SRem => crate::ir::core_inst::BinaryOpcode::SRem,
                    LegacyBinaryOpcode::Shl => crate::ir::core_inst::BinaryOpcode::Shl,
                    LegacyBinaryOpcode::LShr => crate::ir::core_inst::BinaryOpcode::LShr,
                    LegacyBinaryOpcode::AShr => crate::ir::core_inst::BinaryOpcode::AShr,
                    LegacyBinaryOpcode::And => crate::ir::core_inst::BinaryOpcode::And,
                    LegacyBinaryOpcode::Or => crate::ir::core_inst::BinaryOpcode::Or,
                    LegacyBinaryOpcode::Xor => crate::ir::core_inst::BinaryOpcode::Xor,
                },
                lhs: self.convert_legacy_value(&operands[0], const_map, global_map, arg_map, inst_map),
                rhs: self.convert_legacy_value(&operands[1], const_map, global_map, arg_map, inst_map),
            },
            LegacyInstructionKind::Call => {
                let func = function_map[&Rc::as_ptr(&operands[0])];
                let args = operands[1..]
                    .iter()
                    .map(|value| {
                        self.convert_legacy_value(value, const_map, global_map, arg_map, inst_map)
                    })
                    .collect();
                InstKind::Call { func, args }
            }
            LegacyInstructionKind::Branch { has_cond } => {
                if *has_cond {
                    let cond = self.convert_legacy_value(
                        &operands[0],
                        const_map,
                        global_map,
                        arg_map,
                        inst_map,
                    );
                    let then_block = block_map[&Rc::as_ptr(&operands[1])].block;
                    let else_block = block_map[&Rc::as_ptr(&operands[2])].block;
                    InstKind::Branch {
                        then_block,
                        cond: Some(crate::ir::core_inst::CondBranch { cond, else_block }),
                    }
                } else {
                    let then_block = block_map[&Rc::as_ptr(&operands[0])].block;
                    InstKind::Branch {
                        then_block,
                        cond: None,
                    }
                }
            }
            LegacyInstructionKind::GetElementPtr { base_ty } => InstKind::GetElementPtr {
                base_ty: base_ty.clone(),
                base: self.convert_legacy_value(&operands[0], const_map, global_map, arg_map, inst_map),
                indices: operands[1..]
                    .iter()
                    .map(|value| {
                        self.convert_legacy_value(value, const_map, global_map, arg_map, inst_map)
                    })
                    .collect(),
            },
            LegacyInstructionKind::Alloca { inner_ty } => InstKind::Alloca { ty: inner_ty.clone() },
            LegacyInstructionKind::Load => InstKind::Load {
                ptr: self.convert_legacy_value(&operands[0], const_map, global_map, arg_map, inst_map),
            },
            LegacyInstructionKind::Ret { is_void } => InstKind::Ret {
                value: if *is_void {
                    None
                } else {
                    Some(self.convert_legacy_value(
                        &operands[0],
                        const_map,
                        global_map,
                        arg_map,
                        inst_map,
                    ))
                },
            },
            LegacyInstructionKind::Store => InstKind::Store {
                value: self.convert_legacy_value(&operands[0], const_map, global_map, arg_map, inst_map),
                ptr: self.convert_legacy_value(&operands[1], const_map, global_map, arg_map, inst_map),
            },
            LegacyInstructionKind::Icmp(op) => InstKind::ICmp {
                op: match op {
                    LegacyICmpCode::Eq => crate::ir::core_inst::ICmpCode::Eq,
                    LegacyICmpCode::Ne => crate::ir::core_inst::ICmpCode::Ne,
                    LegacyICmpCode::Ugt => crate::ir::core_inst::ICmpCode::Ugt,
                    LegacyICmpCode::Uge => crate::ir::core_inst::ICmpCode::Uge,
                    LegacyICmpCode::Ult => crate::ir::core_inst::ICmpCode::Ult,
                    LegacyICmpCode::Ule => crate::ir::core_inst::ICmpCode::Ule,
                    LegacyICmpCode::Sgt => crate::ir::core_inst::ICmpCode::Sgt,
                    LegacyICmpCode::Sge => crate::ir::core_inst::ICmpCode::Sge,
                    LegacyICmpCode::Slt => crate::ir::core_inst::ICmpCode::Slt,
                    LegacyICmpCode::Sle => crate::ir::core_inst::ICmpCode::Sle,
                },
                lhs: self.convert_legacy_value(&operands[0], const_map, global_map, arg_map, inst_map),
                rhs: self.convert_legacy_value(&operands[1], const_map, global_map, arg_map, inst_map),
            },
            LegacyInstructionKind::Phi => InstKind::Phi {
                incomings: operands
                    .chunks(2)
                    .map(|pair| PhiIncoming {
                        value: self.convert_legacy_value(&pair[0], const_map, global_map, arg_map, inst_map),
                        block: block_map[&Rc::as_ptr(&pair[1])].block,
                    })
                    .collect(),
            },
            LegacyInstructionKind::Select => InstKind::Select {
                cond: self.convert_legacy_value(&operands[0], const_map, global_map, arg_map, inst_map),
                then_val: self.convert_legacy_value(&operands[1], const_map, global_map, arg_map, inst_map),
                else_val: self.convert_legacy_value(&operands[2], const_map, global_map, arg_map, inst_map),
            },
            LegacyInstructionKind::PtrToInt => InstKind::PtrToInt {
                ptr: self.convert_legacy_value(&operands[0], const_map, global_map, arg_map, inst_map),
            },
            LegacyInstructionKind::Trunc => InstKind::Trunc {
                value: self.convert_legacy_value(&operands[0], const_map, global_map, arg_map, inst_map),
            },
            LegacyInstructionKind::Zext => InstKind::Zext {
                value: self.convert_legacy_value(&operands[0], const_map, global_map, arg_map, inst_map),
            },
            LegacyInstructionKind::Sext => InstKind::Sext {
                value: self.convert_legacy_value(&operands[0], const_map, global_map, arg_map, inst_map),
            },
            LegacyInstructionKind::Unreachable => InstKind::Unreachable,
        }
    }
}
