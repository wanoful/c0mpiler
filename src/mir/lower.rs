pub(crate) mod branch_relax;
pub(crate) mod layout;
pub(crate) mod logue;
pub(crate) mod phi;
pub(crate) mod regalloc;

use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt::{self, Display, Formatter},
    marker::PhantomData,
};

use crate::{
    ir::{
        core::{BlockRef, FunctionId, InstRef, ModuleCore, ValueId},
        core_inst::{BinaryOpcode, ICmpCode, InstKind},
        core_value::{ConstKind, GlobalKind},
        ir_type::TypePtr,
        layout::{LayoutShape, TypeLayout, TypeLayoutEngine},
    },
    mir::{
        BlockId, ControlFlowGraph, FrameInfo, FrameLayout, Linkage, LivenessInfo, LoweringTarget,
        MachineBlock, MachineFunction, MachineModule, MachineSegment, MachineSymbolKind, Register,
        StackSlotId, SymbolId, TargetArch, VRegCounter, VRegId, lower::phi::PhiInfo,
    },
};

use crate::mir::TargetInst;

#[derive(Debug, Clone, Copy, Default)]
pub struct LowerOptions {
    pub lower_function_bodies: bool,
    pub need_branch_relaxation: bool,
    pub optimize_fallthroughs: bool,
}

#[derive(Debug, Clone)]
pub enum LowerError {
    DuplicateSymbol(String),
    MissingFunctionName,
    MissingEntryBlock {
        function: String,
    },
    UnknownFunctionSymbol(String),
    UnknownBlock {
        function: String,
        block_name: Option<String>,
    },
    UnimplementedGlobal(String),
    UnimplementedInstruction {
        function: String,
        opcode: String,
    },
    TypeLayoutError(String),
    UnknownOperand {
        function: String,
        operand: String,
    },
}

impl Display for LowerError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            LowerError::DuplicateSymbol(name) => write!(f, "duplicate machine symbol `{name}`"),
            LowerError::MissingFunctionName => write!(f, "function value is missing a symbol name"),
            LowerError::MissingEntryBlock { function } => {
                write!(f, "function `{function}` does not contain an entry block")
            }
            LowerError::UnknownFunctionSymbol(name) => {
                write!(
                    f,
                    "function symbol `{name}` was not registered before lowering"
                )
            }
            LowerError::UnknownBlock {
                function,
                block_name,
            } => match block_name {
                Some(block_name) => {
                    write!(f, "unknown block `{block_name}` in function `{function}`")
                }
                None => write!(f, "unknown unnamed block in function `{function}`"),
            },
            LowerError::UnimplementedGlobal(name) => {
                write!(
                    f,
                    "global lowering for `{name}` has not been implemented yet"
                )
            }
            LowerError::UnimplementedInstruction { function, opcode } => write!(
                f,
                "instruction lowering for `{opcode}` in function `{function}` has not been implemented yet"
            ),
            LowerError::TypeLayoutError(e) => write!(f, "type layout error: {e}"),
            LowerError::UnknownOperand { function, operand } => {
                write!(f, "unknown operand `{operand}` in function `{function}`")
            }
        }
    }
}

impl Error for LowerError {}

fn type_layout(module: &ModuleCore, ty: &TypePtr) -> Result<TypeLayout, LowerError> {
    let target = module
        .target_data_layout()
        .ok_or_else(|| LowerError::TypeLayoutError("missing target data layout".to_string()))?;
    TypeLayoutEngine::new(target)
        .layout_of(ty)
        .map_err(|e| LowerError::TypeLayoutError(e.to_string()))
}

fn value_name(module: &ModuleCore, value: ValueId) -> Option<String> {
    match value {
        ValueId::Inst(inst) => module.inst(inst).name.clone(),
        ValueId::Arg(arg) => module.arg(arg).name.clone(),
        ValueId::Global(global) => Some(module.global(global).name.clone()),
        ValueId::Const(..) => None,
    }
}

fn block_name(module: &ModuleCore, block: BlockRef) -> Option<String> {
    module.block(block).name.clone()
}

fn lower_operand<R: LoweringTarget>(
    module: &ModuleCore,
    operand: ValueId,
    out: &mut Vec<R::MachineInst>,
    state: &FunctionLoweringState,
    machine_function: &mut MachineFunction<R>,
    machine_module: &MachineModule<R>,
) -> Result<Register<R::PhysicalReg>, LowerError> {
    if let ValueId::Global(global) = operand {
        let name = module.global(global).name.clone();
        let symbol_id = machine_module.symbol_map[&name];
        let rd = Register::Virtual(machine_function.new_vreg());
        out.push(R::emit_load_symbol_addr(rd, symbol_id));
        return Ok(rd);
    }

    if let ValueId::Const(constant) = operand {
        match &module.const_data(constant).kind {
            ConstKind::Int(number) => {
                let vreg = machine_function.new_vreg();
                out.push(R::MachineInst::load_imm(
                    Register::Virtual(vreg),
                    *number as i32,
                ));
                return Ok(Register::Virtual(vreg));
            }
            ConstKind::Null => {
                let vreg = machine_function.new_vreg();
                out.push(R::MachineInst::load_imm(Register::Virtual(vreg), 0));
                return Ok(Register::Virtual(vreg));
            }
            ConstKind::Undef => return Ok(Register::Physical(R::zero_reg())),
            _ => {
                return Err(LowerError::UnimplementedGlobal(
                    "non-scalar constant operand lowering is not implemented yet".to_string(),
                ));
            }
        }
    }

    if let Some(vreg) = state.vreg_for_value(operand) {
        Ok(Register::Virtual(vreg))
    } else {
        Err(LowerError::UnknownOperand {
            function: state.function_name.clone(),
            operand: value_name(module, operand).unwrap_or_else(|| "unknown".into()),
        })
    }
}

struct CopyMove<R: TargetArch> {
    dst: Register<R::PhysicalReg>,
    src: Register<R::PhysicalReg>,
}

fn resolve_parallel_copy<R: TargetArch>(
    moves: Vec<CopyMove<R>>,
    machine_function: &mut MachineFunction<R>,
) -> Vec<R::MachineInst> {
    let mut edges: HashMap<Register<R::PhysicalReg>, Vec<Register<R::PhysicalReg>>> =
        HashMap::new();
    for CopyMove { dst, src } in moves {
        if dst != src {
            edges.entry(src).or_default().push(dst);
        }
    }

    let mut visited = HashSet::new();
    let mut visiting = HashSet::new();

    struct Insts<R: TargetArch> {
        middle: Vec<R::MachineInst>,
        first: Vec<R::MachineInst>,
        last: Vec<R::MachineInst>,
    }

    fn visit_fn<R: TargetArch>(
        node: Register<R::PhysicalReg>,
        visited: &mut HashSet<Register<R::PhysicalReg>>,
        visiting: &mut HashSet<Register<R::PhysicalReg>>,
        edges: &HashMap<Register<R::PhysicalReg>, Vec<Register<R::PhysicalReg>>>,
        insts: &mut Insts<R>,
        machine_function: &mut MachineFunction<R>,
    ) {
        if visited.contains(&node) {
            return;
        }
        visiting.insert(node);
        visited.insert(node);

        if let Some(neighbors) = edges.get(&node) {
            for &neighbor in neighbors {
                visit_fn(neighbor, visited, visiting, edges, insts, machine_function);
                if visiting.contains(&neighbor) {
                    let temp = Register::Virtual(machine_function.new_vreg());
                    insts.first.push(R::MachineInst::mv(temp, node));
                    insts.last.push(R::MachineInst::mv(neighbor, temp));
                } else {
                    insts.middle.push(R::MachineInst::mv(neighbor, node));
                }
            }
        }

        visiting.remove(&node);
    }

    let nodes: Vec<_> = edges.keys().copied().collect();
    let mut insts = Insts {
        middle: Vec::new(),
        first: Vec::new(),
        last: Vec::new(),
    };
    for node in nodes {
        visit_fn(
            node,
            &mut visited,
            &mut visiting,
            &edges,
            &mut insts,
            machine_function,
        );
    }

    let mut out = Vec::new();
    out.extend(insts.first);
    out.extend(insts.middle);
    out.extend(insts.last);
    out
}

fn parallel_copy<R: LoweringTarget>(
    module: &ModuleCore,
    phis: Vec<(VRegId, ValueId)>,
    out: &mut Vec<R::MachineInst>,
    state: &FunctionLoweringState,
    machine_function: &mut MachineFunction<R>,
    machine_module: &MachineModule<R>,
) -> Result<(), LowerError> {
    let mut moves = Vec::with_capacity(phis.len());
    for (dst, value) in phis {
        let src = lower_operand(module, value, out, state, machine_function, machine_module)?;
        moves.push(CopyMove {
            dst: Register::Virtual(dst),
            src,
        });
    }
    out.extend(resolve_parallel_copy(moves, machine_function));
    Ok(())
}

fn emit_masked_value<T: LoweringTarget>(
    src: Register<T::PhysicalReg>,
    bits: u8,
    out: &mut Vec<T::MachineInst>,
    machine_function: &mut MachineFunction<T>,
) -> Register<T::PhysicalReg> {
    if bits >= 32 {
        return src;
    }

    let rd = Register::Virtual(machine_function.new_vreg());
    let mask = (1u32 << bits) - 1;
    if mask <= 0x7ff {
        out.push(T::emit_andi(rd, src, mask as i32));
    } else {
        let mask_reg = Register::Virtual(machine_function.new_vreg());
        out.push(T::MachineInst::load_imm(mask_reg, mask as i32));
        out.push(T::emit_and(rd, src, mask_reg));
    }

    rd
}

fn emit_add_offset<T: LoweringTarget>(
    module: &ModuleCore,
    base: Register<T::PhysicalReg>,
    stride: u32,
    index: ValueId,
    out: &mut Vec<T::MachineInst>,
    machine_function: &mut MachineFunction<T>,
    state: &mut FunctionLoweringState,
    machine_module: &MachineModule<T>,
) -> Result<Register<T::PhysicalReg>, LowerError> {
    if let ValueId::Const(constant) = index
        && let ConstKind::Int(number) = &module.const_data(constant).kind
    {
        let imm = (*number as i32).checked_mul(stride as i32).unwrap();
        if (-2048..=2047).contains(&imm) {
            let result_reg = Register::Virtual(machine_function.new_vreg());
            out.push(T::emit_addi(result_reg, base, imm));
            return Ok(result_reg);
        }

        let temp_reg = Register::Virtual(machine_function.new_vreg());
        out.push(T::MachineInst::load_imm(temp_reg, imm));
        let result_reg = Register::Virtual(machine_function.new_vreg());
        out.push(T::emit_add(result_reg, base, temp_reg));
        return Ok(result_reg);
    }

    if stride == 0 {
        return Ok(base);
    }

    let index_reg = lower_operand(module, index, out, state, machine_function, machine_module)?;
    let temp_reg = Register::Virtual(machine_function.new_vreg());
    if stride.is_power_of_two() {
        out.push(T::emit_slli(
            temp_reg,
            index_reg,
            stride.trailing_zeros() as i32,
        ));
    } else {
        let stride_reg = Register::Virtual(machine_function.new_vreg());
        out.push(T::MachineInst::load_imm(stride_reg, stride as i32));
        out.push(T::emit_mul(temp_reg, index_reg, stride_reg));
    }
    let result_reg = Register::Virtual(machine_function.new_vreg());
    out.push(T::emit_add(result_reg, base, temp_reg));
    Ok(result_reg)
}

fn emit_icmp<T: LoweringTarget>(
    icmp_code: &ICmpCode,
    rd: Register<T::PhysicalReg>,
    rs1: Register<T::PhysicalReg>,
    rs2: Register<T::PhysicalReg>,
    out: &mut Vec<T::MachineInst>,
    machine_function: &mut MachineFunction<T>,
) {
    match icmp_code {
        ICmpCode::Eq => {
            let tmp = Register::Virtual(machine_function.new_vreg());
            out.push(T::emit_xor(tmp, rs1, rs2));
            out.push(T::emit_sltiu(rd, tmp, 1));
        }
        ICmpCode::Ne => {
            let tmp = Register::Virtual(machine_function.new_vreg());
            out.push(T::emit_xor(tmp, rs1, rs2));
            out.push(T::emit_sltu(rd, Register::Physical(T::zero_reg()), tmp));
        }
        ICmpCode::Ugt => out.push(T::emit_sltu(rd, rs2, rs1)),
        ICmpCode::Uge => {
            let tmp = Register::Virtual(machine_function.new_vreg());
            out.push(T::emit_sltu(tmp, rs1, rs2));
            out.push(T::emit_xori(rd, tmp, 1));
        }
        ICmpCode::Ult => out.push(T::emit_sltu(rd, rs1, rs2)),
        ICmpCode::Ule => {
            let tmp = Register::Virtual(machine_function.new_vreg());
            out.push(T::emit_sltu(tmp, rs2, rs1));
            out.push(T::emit_xori(rd, tmp, 1));
        }
        ICmpCode::Sgt => out.push(T::emit_slt(rd, rs2, rs1)),
        ICmpCode::Sge => {
            let tmp = Register::Virtual(machine_function.new_vreg());
            out.push(T::emit_slt(tmp, rs1, rs2));
            out.push(T::emit_xori(rd, tmp, 1));
        }
        ICmpCode::Slt => out.push(T::emit_slt(rd, rs1, rs2)),
        ICmpCode::Sle => {
            let tmp = Register::Virtual(machine_function.new_vreg());
            out.push(T::emit_slt(tmp, rs2, rs1));
            out.push(T::emit_xori(rd, tmp, 1));
        }
    }
}

#[derive(Debug, Default)]
struct ModuleLoweringState {
    function_symbols: HashMap<String, SymbolId>,
    global_symbols: HashMap<String, SymbolId>,
}

#[derive(Debug)]
pub(crate) struct FunctionLoweringState {
    function_name: String,
    block_map: HashMap<BlockRef, BlockId>,
    block_order: Vec<BlockRef>,
    value_vregs: HashMap<ValueId, VRegId>,
    stack_slots: HashMap<InstRef, StackSlotId>,
    phi_infos: HashMap<BlockId, Vec<PhiInfo>>,
}

impl FunctionLoweringState {
    fn new(function_name: String) -> Self {
        Self {
            function_name,
            block_map: HashMap::new(),
            block_order: Vec::new(),
            value_vregs: HashMap::new(),
            stack_slots: HashMap::new(),
            phi_infos: HashMap::new(),
        }
    }

    fn record_block(&mut self, block: BlockRef, id: BlockId) {
        self.block_map.insert(block, id);
        self.block_order.push(block);
    }

    fn block_id(&self, block: &BlockRef) -> Option<BlockId> {
        self.block_map.get(block).copied()
    }

    fn record_vreg(&mut self, value: ValueId, vreg: VRegId) {
        self.value_vregs.insert(value, vreg);
    }

    fn vreg_for_value(&self, value: ValueId) -> Option<VRegId> {
        self.value_vregs.get(&value).copied()
    }
}

#[derive(Debug)]
pub struct Lowerer<T: LoweringTarget> {
    options: LowerOptions,
    _marker: PhantomData<T>,
}

impl<T: LoweringTarget> Default for Lowerer<T> {
    fn default() -> Self {
        Self {
            options: LowerOptions::default(),
            _marker: PhantomData,
        }
    }
}

pub type RV32Lowerer = Lowerer<crate::mir::rv32im::RV32Arch>;

impl<T: LoweringTarget> Lowerer<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_options(options: LowerOptions) -> Self {
        Self {
            options,
            _marker: PhantomData,
        }
    }

    pub fn lower_module(&mut self, module: &ModuleCore) -> Result<MachineModule<T>, LowerError> {
        let mut machine_module = MachineModule::default();
        let mut module_state = ModuleLoweringState::default();

        self.collect_symbols(module, &mut machine_module, &mut module_state)?;
        self.lower_globals(module, &mut machine_module, &module_state)?;

        if self.options.lower_function_bodies {
            self.lower_functions(module, &mut machine_module, &module_state)?;
        }

        Ok(machine_module)
    }

    fn collect_symbols(
        &mut self,
        module: &ModuleCore,
        machine_module: &mut MachineModule<T>,
        state: &mut ModuleLoweringState,
    ) -> Result<(), LowerError> {
        for global in module.globals_in_order() {
            let global_data = module.global(global);
            if matches!(global_data.kind, GlobalKind::Function(_)) {
                continue;
            }
            let name = global_data.name.clone();
            self.ensure_symbol_absent(machine_module, &name)?;

            let symbol = machine_module.new_symbol(
                name.clone(),
                MachineSymbolKind::ExternalPlaceholder,
                MachineSegment::Data,
                Linkage::Internal,
                1,
            );
            state.global_symbols.insert(name, symbol);
        }

        for function in module.functions_in_order() {
            let name = module.func(function).name.clone();
            self.ensure_symbol_absent(machine_module, &name)?;

            let is_external = module.func(function).is_declare;
            let is_entry_main = name == "main";
            let linkage = if is_external || is_entry_main {
                Linkage::External
            } else {
                Linkage::Internal
            };
            let kind = if is_external {
                MachineSymbolKind::ExternalPlaceholder
            } else {
                MachineSymbolKind::Function(empty_machine_function(name.clone()))
            };

            let symbol =
                machine_module.new_symbol(name.clone(), kind, MachineSegment::Text, linkage, 4);
            state.function_symbols.insert(name, symbol);
        }

        Ok(())
    }

    fn lower_globals(
        &mut self,
        module: &ModuleCore,
        machine_module: &mut MachineModule<T>,
        state: &ModuleLoweringState,
    ) -> Result<(), LowerError> {
        for global in module.globals_in_order() {
            let global_data = module.global(global);
            if matches!(global_data.kind, GlobalKind::Function(_)) {
                continue;
            }
            let name = global_data.name.clone();
            let symbol_id = state.global_symbols[&name];
            let symbol = &mut machine_module.symbols[symbol_id.0];
            symbol.kind = self.lower_global(module, global)?;
            if let GlobalKind::GlobalVariable { is_constant, .. } = global_data.kind
                && is_constant
            {
                symbol.segment = MachineSegment::ReadOnlyData;
            }
            symbol.alignment = type_layout(module, &global_data.ty)?.layout.align as usize;
        }

        Ok(())
    }

    fn lower_functions(
        &mut self,
        module: &ModuleCore,
        machine_module: &mut MachineModule<T>,
        state: &ModuleLoweringState,
    ) -> Result<(), LowerError> {
        for function in module.functions_in_order() {
            if module.func(function).is_declare {
                continue;
            }

            let name = module.func(function).name.clone();
            let symbol_id = state.function_symbols[&name];
            let lowered = self.lower_function(function, module, machine_module)?;
            machine_module.symbols[symbol_id.0].kind = MachineSymbolKind::Function(lowered);
        }

        Ok(())
    }

    fn lower_global(
        &mut self,
        module: &ModuleCore,
        global: crate::ir::core::GlobalId,
    ) -> Result<MachineSymbolKind<T>, LowerError> {
        let global_data = module.global(global);
        let initializer = match global_data.kind {
            GlobalKind::GlobalVariable { initializer, .. } => initializer,
            GlobalKind::Function(_) => {
                return Err(LowerError::UnimplementedGlobal(global_data.name.clone()));
            }
        };

        Ok(MachineSymbolKind::Data(lower_constant(
            module,
            &global_data.ty,
            initializer,
        )?))
    }

    fn lower_function(
        &mut self,
        function: FunctionId,
        module: &ModuleCore,
        machine_module: &mut MachineModule<T>,
    ) -> Result<Box<MachineFunction<T>>, LowerError> {
        let function_name = module.func(function).name.clone();
        let blocks = module.blocks_in_order(function);
        let entry = module
            .entry_block(function)
            .ok_or_else(|| LowerError::MissingEntryBlock {
                function: function_name.clone(),
            })?;

        let mut state = FunctionLoweringState::new(function_name.clone());
        let mut machine_function = empty_machine_function(function_name.clone());

        machine_function.entry = BlockId(0);
        self.initialize_blocks(module, &blocks, &mut machine_function, &mut state)?;
        self.collect_phis(module, &mut machine_function, &mut state)?;
        self.initialize_func_arguments(function, module, &mut machine_function, &mut state);

        machine_function.entry = state.block_id(&entry).unwrap_or_else(|| {
            panic!(
                "entry block should have been initialized: {:?}.\nFunction: {}\n State: {:#?}",
                entry, function_name, state
            )
        });

        for &block in blocks.iter() {
            self.collect_value(block, &mut machine_function, module, &mut state);
        }

        for &block in blocks.iter() {
            self.lower_block(
                block,
                &mut machine_function,
                module,
                machine_module,
                &mut state,
            )?;
        }

        self.register_allocation(&mut machine_function);
        self.compute_frame_layout(&mut machine_function);
        self.insert_logue(&mut machine_function);
        self.expand_pseudo_instructions(&mut machine_function);

        if self.options.optimize_fallthroughs {
            self.optimize_fallthroughs(&mut machine_function);
        }

        if self.options.need_branch_relaxation {
            self.relax_branches(&mut machine_function)?;
        }

        Ok(machine_function)
    }

    fn initialize_blocks(
        &mut self,
        module: &ModuleCore,
        blocks: &[BlockRef],
        machine_function: &mut MachineFunction<T>,
        state: &mut FunctionLoweringState,
    ) -> Result<(), LowerError> {
        for (index, block) in blocks.iter().enumerate() {
            let id = BlockId(index);
            let name = block_name(module, *block).unwrap_or_else(|| format!(".bb{index}"));
            machine_function.blocks.push(MachineBlock {
                id,
                name,
                instructions: Vec::new(),
            });
            state.record_block(*block, id);
        }

        Ok(())
    }

    fn collect_value(
        &mut self,
        block: BlockRef,
        machine_function: &mut MachineFunction<T>,
        module: &ModuleCore,
        state: &mut FunctionLoweringState,
    ) {
        for instruction in module.phis_in_order(block) {
            if !module.value_ty(ValueId::Inst(instruction)).is_void() {
                self.ensure_inst_vreg(instruction, machine_function, state);
            }
        }

        for instruction in module.insts_in_order(block) {
            if !module.value_ty(ValueId::Inst(instruction)).is_void() {
                self.ensure_inst_vreg(instruction, machine_function, state);
            }
        }

        if let Some(instruction) = module.terminator(block)
            && !module.value_ty(ValueId::Inst(instruction)).is_void()
        {
            self.ensure_inst_vreg(instruction, machine_function, state);
        }
    }

    fn ensure_inst_vreg(
        &self,
        instruction: InstRef,
        machine_function: &mut MachineFunction<T>,
        state: &mut FunctionLoweringState,
    ) -> VRegId {
        let value = ValueId::Inst(instruction);
        if let Some(vreg) = state.vreg_for_value(value) {
            vreg
        } else {
            let vreg = machine_function.new_vreg();
            state.record_vreg(value, vreg);
            vreg
        }
    }

    fn lower_block(
        &mut self,
        block: BlockRef,
        machine_function: &mut MachineFunction<T>,
        module: &ModuleCore,
        machine_module: &mut MachineModule<T>,
        state: &mut FunctionLoweringState,
    ) -> Result<(), LowerError> {
        let block_id = state.block_id(&block).unwrap();
        for instruction in module.phis_in_order(block) {
            let insts = self.lower_instruction(
                instruction,
                block,
                block_id,
                module,
                machine_function,
                machine_module,
                state,
            )?;
            machine_function
                .get_block_mut(block_id)
                .unwrap()
                .instructions
                .extend(insts);
        }

        for instruction in module.insts_in_order(block) {
            let insts = self.lower_instruction(
                instruction,
                block,
                block_id,
                module,
                machine_function,
                machine_module,
                state,
            )?;
            machine_function
                .get_block_mut(block_id)
                .unwrap()
                .instructions
                .extend(insts);
        }

        if let Some(instruction) = module.terminator(block) {
            let insts = self.lower_instruction(
                instruction,
                block,
                block_id,
                module,
                machine_function,
                machine_module,
                state,
            )?;
            machine_function
                .get_block_mut(block_id)
                .unwrap()
                .instructions
                .extend(insts);
        }

        Ok(())
    }

    fn lower_instruction(
        &mut self,
        instruction: InstRef,
        block: BlockRef,
        block_id: BlockId,
        module: &ModuleCore,
        machine_function: &mut MachineFunction<T>,
        machine_module: &mut MachineModule<T>,
        state: &mut FunctionLoweringState,
    ) -> Result<Vec<T::MachineInst>, LowerError> {
        let inst = module.inst(instruction);

        let mut out = Vec::new();

        match &inst.kind {
            InstKind::Binary { op, lhs, rhs } => {
                let rs1 = lower_operand(
                    module,
                    *lhs,
                    &mut out,
                    state,
                    machine_function,
                    machine_module,
                )?;

                if let ValueId::Const(constant) = *rhs
                    && let ConstKind::Int(number) = &module.const_data(constant).kind
                    && matches!(
                        op,
                        BinaryOpcode::Add
                            | BinaryOpcode::Sub
                            | BinaryOpcode::Shl
                            | BinaryOpcode::LShr
                            | BinaryOpcode::AShr
                            | BinaryOpcode::And
                            | BinaryOpcode::Or
                            | BinaryOpcode::Xor
                    )
                {
                    let rd_vreg = self.ensure_inst_vreg(instruction, machine_function, state);
                    let rd = Register::Virtual(rd_vreg);
                    let imm = *number as i32;
                    let lowered = match op {
                        BinaryOpcode::Add if (-2048..=2047).contains(&imm) => {
                            Some(T::emit_addi(rd, rs1, imm))
                        }
                        BinaryOpcode::Sub => imm
                            .checked_neg()
                            .filter(|neg_imm| (-2048..=2047).contains(neg_imm))
                            .map(|neg_imm| T::emit_addi(rd, rs1, neg_imm)),
                        BinaryOpcode::Shl if (0..32).contains(&imm) => {
                            Some(T::emit_slli(rd, rs1, imm))
                        }
                        BinaryOpcode::LShr if (0..32).contains(&imm) => {
                            Some(T::emit_srli(rd, rs1, imm))
                        }
                        BinaryOpcode::AShr if (0..32).contains(&imm) => {
                            Some(T::emit_srai(rd, rs1, imm))
                        }
                        BinaryOpcode::And if (-2048..=2047).contains(&imm) => {
                            Some(T::emit_andi(rd, rs1, imm))
                        }
                        BinaryOpcode::Or if (-2048..=2047).contains(&imm) => {
                            Some(T::emit_ori(rd, rs1, imm))
                        }
                        BinaryOpcode::Xor if (-2048..=2047).contains(&imm) => {
                            Some(T::emit_xori(rd, rs1, imm))
                        }
                        _ => None,
                    };

                    if let Some(lowered) = lowered {
                        out.push(lowered);
                        return Ok(out);
                    }
                }

                let rs2 = lower_operand(
                    module,
                    *rhs,
                    &mut out,
                    state,
                    machine_function,
                    machine_module,
                )?;
                let rd_vreg = self.ensure_inst_vreg(instruction, machine_function, state);
                let rd = Register::Virtual(rd_vreg);

                let lowered = match op {
                    BinaryOpcode::Add => T::emit_add(rd, rs1, rs2),
                    BinaryOpcode::Sub => T::emit_sub(rd, rs1, rs2),
                    BinaryOpcode::Mul => T::emit_mul(rd, rs1, rs2),
                    BinaryOpcode::UDiv => T::emit_divu(rd, rs1, rs2),
                    BinaryOpcode::SDiv => T::emit_div(rd, rs1, rs2),
                    BinaryOpcode::URem => T::emit_remu(rd, rs1, rs2),
                    BinaryOpcode::SRem => T::emit_rem(rd, rs1, rs2),
                    BinaryOpcode::And => T::emit_and(rd, rs1, rs2),
                    BinaryOpcode::Or => T::emit_or(rd, rs1, rs2),
                    BinaryOpcode::Xor => T::emit_xor(rd, rs1, rs2),
                    BinaryOpcode::Shl => T::emit_sll(rd, rs1, rs2),
                    BinaryOpcode::LShr => T::emit_srl(rd, rs1, rs2),
                    BinaryOpcode::AShr => T::emit_sra(rd, rs1, rs2),
                };

                out.push(lowered);
            }
            InstKind::Call { func, args } => {
                let raw_callee_name = module.func(*func).name.clone();
                let (callee_name, args) = if raw_callee_name.starts_with("llvm.memcpy.")
                    || raw_callee_name.starts_with("llvm.memmove.")
                {
                    let mem_args = if args.len() >= 4 {
                        &args[..3]
                    } else {
                        &args[..]
                    };
                    let libc_symbol = if raw_callee_name.starts_with("llvm.memmove.") {
                        "memmove"
                    } else {
                        "memcpy"
                    };
                    (libc_symbol.to_string(), mem_args.to_vec())
                } else {
                    (raw_callee_name, args.clone())
                };

                let func_id = if let Some(symbol_id) = machine_module.symbol_map.get(&callee_name) {
                    *symbol_id
                } else {
                    machine_module.new_symbol(
                        callee_name,
                        MachineSymbolKind::ExternalPlaceholder,
                        MachineSegment::Text,
                        Linkage::External,
                        4,
                    )
                };
                let num_args = args.len();
                let stack_arg_size =
                    num_args.saturating_sub(T::num_arg_regs()) * T::stack_arg_size();
                machine_function.record_outgoing_arg(stack_arg_size);

                let lowered_args = args
                    .iter()
                    .map(|arg| {
                        lower_operand(
                            module,
                            *arg,
                            &mut out,
                            state,
                            machine_function,
                            machine_module,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                let mut register_moves = Vec::new();
                for (index, rs) in lowered_args.into_iter().enumerate() {
                    if index < T::num_arg_regs() {
                        register_moves.push(CopyMove {
                            dst: Register::Physical(T::arg_reg(index)),
                            src: rs,
                        });
                    } else {
                        let rt = Register::Virtual(machine_function.new_vreg());
                        out.push(T::emit_store_outgoing_arg(
                            rs,
                            T::stack_arg_offset(index - T::num_arg_regs()),
                            rt,
                        ));
                    }
                }

                out.extend(resolve_parallel_copy(register_moves, machine_function));

                out.push(T::emit_call(func_id, num_args));

                if !module.value_ty(ValueId::Inst(instruction)).is_void() {
                    let rd_vreg = self.ensure_inst_vreg(instruction, machine_function, state);
                    out.push(T::MachineInst::mv(
                        Register::Virtual(rd_vreg),
                        Register::Physical(T::return_reg()),
                    ));
                }
            }
            InstKind::Branch { then_block, cond } => {
                let collect_phis_from_info = |inner: &Vec<PhiInfo>| {
                    inner
                        .iter()
                        .filter_map(|info| {
                            info.filter_pred(block_id).map(|ptr| (info.get_dst(), ptr))
                        })
                        .collect()
                };

                if let Some(cond_branch) = cond {
                    let cond = lower_operand(
                        module,
                        cond_branch.cond,
                        &mut out,
                        state,
                        machine_function,
                        machine_module,
                    )?;
                    let true_block = BlockRef {
                        func: block.func,
                        block: *then_block,
                    };
                    let false_block = BlockRef {
                        func: block.func,
                        block: cond_branch.else_block,
                    };
                    let mut true_block_id = state.block_id(&true_block).unwrap();
                    let mut false_block_id = state.block_id(&false_block).unwrap();

                    let true_block_phis: Option<Vec<_>> = state
                        .phi_infos
                        .get(&true_block_id)
                        .map(collect_phis_from_info);
                    let false_block_phis: Option<Vec<_>> = state
                        .phi_infos
                        .get(&false_block_id)
                        .map(collect_phis_from_info);

                    let mut add_transit_block =
                        |target_block_id: BlockId,
                         target_block_phis: Vec<(VRegId, ValueId)>|
                         -> Result<BlockId, LowerError> {
                            let transit_block_id = machine_function.blocks.len();
                            let mut transit_insts: Vec<T::MachineInst> = Vec::new();

                            parallel_copy(
                                module,
                                target_block_phis,
                                &mut transit_insts,
                                state,
                                machine_function,
                                machine_module,
                            )?;
                            transit_insts.push(T::emit_jump(target_block_id));
                            let transit_block = MachineBlock {
                                id: BlockId(transit_block_id),
                                name: format!(".bb{transit_block_id}"),
                                instructions: transit_insts,
                            };
                            machine_function.blocks.push(transit_block);
                            Ok(BlockId(transit_block_id))
                        };

                    if let Some(true_block_phis) = &true_block_phis
                        && !true_block_phis.is_empty()
                    {
                        true_block_id = add_transit_block(true_block_id, true_block_phis.clone())?;
                    };

                    if let Some(false_block_phis) = &false_block_phis
                        && !false_block_phis.is_empty()
                    {
                        false_block_id =
                            add_transit_block(false_block_id, false_block_phis.clone())?;
                    };

                    out.push(T::emit_branch_ne(
                        cond,
                        Register::Physical(T::zero_reg()),
                        true_block_id,
                    ));
                    out.push(T::emit_jump(false_block_id));
                } else {
                    let target_block = BlockRef {
                        func: block.func,
                        block: *then_block,
                    };
                    let target_block_id = state.block_id(&target_block).unwrap();

                    let target_block_phis: Option<Vec<_>> = state
                        .phi_infos
                        .get(&target_block_id)
                        .map(collect_phis_from_info);

                    if let Some(target_block_phis) = target_block_phis
                        && !target_block_phis.is_empty()
                    {
                        parallel_copy(
                            module,
                            target_block_phis,
                            &mut out,
                            state,
                            machine_function,
                            machine_module,
                        )?;
                    }

                    out.push(T::emit_jump(target_block_id));
                }
            }
            InstKind::GetElementPtr {
                base_ty,
                base,
                indices,
            } => {
                let mut base = lower_operand(
                    module,
                    *base,
                    &mut out,
                    state,
                    machine_function,
                    machine_module,
                )?;
                let mut ty = base_ty.clone();
                if let Some((base_index, rest_indices)) = indices.split_first() {
                    let base_layout = type_layout(module, &ty)?;
                    base = emit_add_offset(
                        module,
                        base,
                        base_layout.layout.stride(),
                        *base_index,
                        &mut out,
                        machine_function,
                        state,
                        machine_module,
                    )?;

                    for index in rest_indices {
                        let current_layout = type_layout(module, &ty)?;
                        match current_layout.shape {
                            LayoutShape::Struct(_) => {
                                let struct_layout = current_layout.shape.as_struct().unwrap();
                                let field_index = if let ValueId::Const(constant) = *index
                                    && let ConstKind::Int(number) =
                                        &module.const_data(constant).kind
                                {
                                    *number as usize
                                } else {
                                    return Err(LowerError::UnimplementedInstruction {
                                        function: state.function_name.clone(),
                                        opcode: "non-constant struct GEP index".to_string(),
                                    });
                                };
                                let field_layout = &struct_layout.fields[field_index];
                                let temp_reg = Register::Virtual(machine_function.new_vreg());
                                if field_layout.offset <= 2047 {
                                    out.push(T::emit_addi(
                                        temp_reg,
                                        base,
                                        field_layout.offset as i32,
                                    ));
                                } else {
                                    let imm_reg = Register::Virtual(machine_function.new_vreg());
                                    out.push(T::MachineInst::load_imm(
                                        imm_reg,
                                        field_layout.offset as i32,
                                    ));
                                    out.push(T::emit_add(temp_reg, base, imm_reg));
                                }
                                base = temp_reg;
                                ty = ty.as_struct().unwrap().get_body().unwrap()[field_index]
                                    .clone();
                            }
                            LayoutShape::Array(_) => {
                                let array_layout = current_layout.shape.as_array().unwrap();
                                base = emit_add_offset(
                                    module,
                                    base,
                                    array_layout.stride,
                                    *index,
                                    &mut out,
                                    machine_function,
                                    state,
                                    machine_module,
                                )?;
                                ty = ty.as_array().unwrap().0.clone();
                            }
                            _ => {
                                return Err(LowerError::UnimplementedInstruction {
                                    function: state.function_name.clone(),
                                    opcode: "GEP on non-struct non-array type".to_string(),
                                });
                            }
                        }
                    }
                }

                let rd_vreg = self.ensure_inst_vreg(instruction, machine_function, state);
                match base {
                    Register::Virtual(vreg) if vreg == rd_vreg => {}
                    _ => out.push(T::MachineInst::mv(Register::Virtual(rd_vreg), base)),
                }
            }
            InstKind::Alloca { ty } => {
                let layout = type_layout(module, ty)?;
                let stack_slot_id = machine_function.new_stack_slot(
                    layout.layout.size as usize,
                    layout.layout.align as usize,
                    crate::mir::StackSlotKind::Alloca,
                );
                state.stack_slots.insert(instruction, stack_slot_id);
                let rd_vreg = self.ensure_inst_vreg(instruction, machine_function, state);
                out.push(T::emit_get_stack_addr(
                    Register::Virtual(rd_vreg),
                    stack_slot_id,
                ));
            }
            InstKind::Load { ptr } => {
                let rd_vreg = self.ensure_inst_vreg(instruction, machine_function, state);
                let ty = module.value_ty(ValueId::Inst(instruction)).clone();
                let type_layout = type_layout(module, &ty)?;
                let rd = Register::Virtual(rd_vreg);
                let (symbol_id, rs1) = if let ValueId::Global(global) = *ptr {
                    let symbol_id = machine_module.symbol_map[&module.global(global).name];
                    (Some(symbol_id), Register::Physical(T::zero_reg()))
                } else {
                    (
                        None,
                        lower_operand(
                            module,
                            *ptr,
                            &mut out,
                            state,
                            machine_function,
                            machine_module,
                        )?,
                    )
                };

                match type_layout.layout.size as usize {
                    1 | 2 | 4 => {
                        if let Some(symbol_id) = symbol_id {
                            out.push(T::emit_load_global(
                                rd,
                                symbol_id,
                                type_layout.layout.size as usize,
                                false,
                            ));
                        } else {
                            out.push(T::emit_load_mem(
                                rd,
                                rs1,
                                0,
                                type_layout.layout.size as usize,
                                type_layout.layout.size < 4,
                            ));
                        }
                    }
                    _ => panic!("unsupported load size"),
                }
            }
            InstKind::Ret { value } => {
                if let Some(value) = value {
                    let rs = lower_operand(
                        module,
                        *value,
                        &mut out,
                        state,
                        machine_function,
                        machine_module,
                    )?;
                    out.push(T::MachineInst::mv(Register::Physical(T::return_reg()), rs));
                }
                out.push(T::emit_ret());
            }
            InstKind::Store { value, ptr } => {
                let rs2 = lower_operand(
                    module,
                    *value,
                    &mut out,
                    state,
                    machine_function,
                    machine_module,
                )?;
                let stored_ty = if matches!(*value, ValueId::Global(_)) {
                    module.value_ty(*ptr)
                } else {
                    module.value_ty(*value)
                };
                let type_layout = type_layout(module, stored_ty)?;
                if let ValueId::Global(global) = *ptr {
                    let symbol = machine_module.symbol_map[&module.global(global).name];
                    match type_layout.layout.size as usize {
                        1 | 2 | 4 => {
                            out.push(T::emit_store_global(
                                rs2,
                                symbol,
                                type_layout.layout.size as usize,
                                Register::Virtual(machine_function.new_vreg()),
                            ));
                        }
                        _ => panic!("unsupported store size"),
                    }
                } else {
                    let rs1 = lower_operand(
                        module,
                        *ptr,
                        &mut out,
                        state,
                        machine_function,
                        machine_module,
                    )?;
                    match type_layout.layout.size as usize {
                        1 | 2 | 4 => {
                            out.push(T::emit_store_mem(
                                rs1,
                                rs2,
                                0,
                                type_layout.layout.size as usize,
                            ));
                        }
                        _ => panic!("unsupported store size"),
                    }
                }
            }
            InstKind::ICmp { op, lhs, rhs } => {
                let rs1 = lower_operand(
                    module,
                    *lhs,
                    &mut out,
                    state,
                    machine_function,
                    machine_module,
                )?;
                let rs2 = lower_operand(
                    module,
                    *rhs,
                    &mut out,
                    state,
                    machine_function,
                    machine_module,
                )?;
                let rd_vreg = self.ensure_inst_vreg(instruction, machine_function, state);
                let rd = Register::Virtual(rd_vreg);
                emit_icmp(op, rd, rs1, rs2, &mut out, machine_function);
            }
            InstKind::Phi { .. } => {
                assert!(
                    state.vreg_for_value(ValueId::Inst(instruction)).is_some(),
                    "phi node should have been assigned a vreg in the first pass"
                );
            }
            InstKind::Select {
                cond,
                then_val,
                else_val,
            } => {
                let cond = lower_operand(
                    module,
                    *cond,
                    &mut out,
                    state,
                    machine_function,
                    machine_module,
                )?;
                let true_val = lower_operand(
                    module,
                    *then_val,
                    &mut out,
                    state,
                    machine_function,
                    machine_module,
                )?;
                let false_val = lower_operand(
                    module,
                    *else_val,
                    &mut out,
                    state,
                    machine_function,
                    machine_module,
                )?;

                let mask1 = machine_function.new_vreg();
                let mask2 = machine_function.new_vreg();
                out.push(T::emit_sub(
                    Register::Virtual(mask1),
                    Register::Physical(T::zero_reg()),
                    cond,
                ));
                out.push(T::emit_xori(
                    Register::Virtual(mask2),
                    Register::Virtual(mask1),
                    -1,
                ));

                let true_part = machine_function.new_vreg();
                let false_part = machine_function.new_vreg();
                out.push(T::emit_and(
                    Register::Virtual(true_part),
                    true_val,
                    Register::Virtual(mask1),
                ));
                out.push(T::emit_and(
                    Register::Virtual(false_part),
                    false_val,
                    Register::Virtual(mask2),
                ));
                let rd_vreg = self.ensure_inst_vreg(instruction, machine_function, state);
                out.push(T::emit_or(
                    Register::Virtual(rd_vreg),
                    Register::Virtual(true_part),
                    Register::Virtual(false_part),
                ));
            }
            InstKind::PtrToInt { ptr } => {
                let src = lower_operand(
                    module,
                    *ptr,
                    &mut out,
                    state,
                    machine_function,
                    machine_module,
                )?;
                let rd_vreg = self.ensure_inst_vreg(instruction, machine_function, state);
                out.push(T::MachineInst::mv(Register::Virtual(rd_vreg), src));
            }
            InstKind::Trunc { value } => {
                let dst_ty = module.value_ty(ValueId::Inst(instruction));
                let src = lower_operand(
                    module,
                    *value,
                    &mut out,
                    state,
                    machine_function,
                    machine_module,
                )?;
                let dst_bits = dst_ty.as_int().unwrap().0;
                let masked = emit_masked_value(src, dst_bits, &mut out, machine_function);
                let rd_vreg = self.ensure_inst_vreg(instruction, machine_function, state);
                if masked != Register::Virtual(rd_vreg) {
                    out.push(T::MachineInst::mv(Register::Virtual(rd_vreg), masked));
                }
            }
            InstKind::Zext { value } => {
                let src = lower_operand(
                    module,
                    *value,
                    &mut out,
                    state,
                    machine_function,
                    machine_module,
                )?;
                let src_bits = module.value_ty(*value).as_int().unwrap().0;
                let extended = emit_masked_value(src, src_bits, &mut out, machine_function);
                let rd_vreg = self.ensure_inst_vreg(instruction, machine_function, state);
                if extended != Register::Virtual(rd_vreg) {
                    out.push(T::MachineInst::mv(Register::Virtual(rd_vreg), extended));
                }
            }
            InstKind::Sext { value } => {
                let src = lower_operand(
                    module,
                    *value,
                    &mut out,
                    state,
                    machine_function,
                    machine_module,
                )?;
                let rd_vreg = self.ensure_inst_vreg(instruction, machine_function, state);
                let src_bits = module.value_ty(*value).as_int().unwrap().0;
                if src_bits >= 32 {
                    out.push(T::MachineInst::mv(Register::Virtual(rd_vreg), src));
                } else {
                    let shift = 32 - src_bits;
                    out.push(T::emit_slli(Register::Virtual(rd_vreg), src, shift as i32));
                    out.push(T::emit_srai(
                        Register::Virtual(rd_vreg),
                        Register::Virtual(rd_vreg),
                        shift as i32,
                    ));
                }
            }
            InstKind::Unreachable => {}
        };
        Ok(out)
    }

    fn ensure_symbol_absent(
        &self,
        machine_module: &MachineModule<T>,
        name: &str,
    ) -> Result<(), LowerError> {
        if machine_module.symbol_map.contains_key(name) {
            Err(LowerError::DuplicateSymbol(name.to_string()))
        } else {
            Ok(())
        }
    }

    fn initialize_func_arguments(
        &self,
        function: FunctionId,
        module: &ModuleCore,
        machine_function: &mut MachineFunction<T>,
        state: &mut FunctionLoweringState,
    ) {
        let params = module.args_in_order(function);

        let mut insts = params
            .iter()
            .enumerate()
            .map(|(index, param)| {
                let vreg = machine_function.new_vreg();
                state.record_vreg(ValueId::Arg(*param), vreg);
                if index < T::num_arg_regs() {
                    T::MachineInst::mv(
                        Register::Virtual(vreg),
                        Register::Physical(T::arg_reg(index)),
                    )
                } else {
                    T::emit_load_incoming_arg(
                        Register::Virtual(vreg),
                        T::stack_arg_offset(index - T::num_arg_regs()),
                    )
                }
            })
            .collect();

        machine_function
            .get_block_mut(machine_function.entry)
            .unwrap()
            .instructions
            .append(&mut insts);
    }

    pub(crate) fn liveness_analysis(
        &self,
        machine_function: &MachineFunction<T>,
    ) -> LivenessInfo<T> {
        let mut liveness_info: LivenessInfo<T> = LivenessInfo::new(machine_function.blocks.iter());
        let cfg = self.compute_cfg(machine_function);

        let mut changed = true;

        while changed {
            changed = false;
            for block in machine_function.blocks.iter().rev() {
                let succs = &cfg.succs[&block.id];
                changed |= liveness_info.update_liveout(block.id, succs.iter());
                changed |= liveness_info.update_livein(block.id);
            }
        }

        liveness_info.compute_live_after(machine_function);
        liveness_info
    }

    pub(crate) fn compute_cfg(&self, machine_function: &MachineFunction<T>) -> ControlFlowGraph {
        let mut succs: HashMap<BlockId, HashSet<BlockId>> = HashMap::new();

        for block in machine_function.blocks.iter() {
            succs.entry(block.id).or_default();
        }

        for block in machine_function.blocks.iter() {
            for inst in block.instructions.iter().rev() {
                if !inst.is_terminator() {
                    break;
                }
                inst.get_successors().into_iter().for_each(|succ| {
                    succs.entry(block.id).or_default().insert(succ);
                });
            }
        }

        ControlFlowGraph { succs }
    }

    fn expand_pseudo_instructions(&self, machine_function: &mut MachineFunction<T>) {
        for block in &mut machine_function.blocks {
            let mut expanded_insts = Vec::new();
            for inst in &block.instructions {
                expanded_insts.extend(T::expand_pseudo(inst, &machine_function.frame_layout));
            }
            block.instructions = expanded_insts;
        }
    }
}

fn lower_constant(
    module: &ModuleCore,
    ty: &TypePtr,
    constant: Option<crate::ir::core::ConstId>,
) -> Result<Vec<u8>, LowerError> {
    let type_layout = type_layout(module, ty)?;
    let size = type_layout.layout.size as usize;

    let mut bytes = match constant {
        Some(constant) => lower_const_data(module, constant)?,
        None => vec![0; size],
    };

    assert!(
        bytes.len() <= size,
        "Constant lowering produced {} bytes, but type layout size is {}",
        bytes.len(),
        size
    );
    bytes.resize(size, 0);
    Ok(bytes)
}

fn lower_const_data(
    module: &ModuleCore,
    constant: crate::ir::core::ConstId,
) -> Result<Vec<u8>, LowerError> {
    let const_data = module.const_data(constant);
    let type_layout = type_layout(module, &const_data.ty)?;

    match &const_data.kind {
        ConstKind::Int(number) => {
            let bytes = number.to_le_bytes();
            Ok(bytes[..(type_layout.layout.size as usize)].to_vec())
        }
        ConstKind::Array(elements) => {
            let array_layout = type_layout.shape.as_array().unwrap();
            let mut bytes = Vec::new();
            for element in elements {
                let mut element_bytes = lower_const_data(module, *element)?;
                assert!(element_bytes.len() <= array_layout.stride as usize);
                element_bytes.resize(array_layout.stride as usize, 0);
                bytes.append(&mut element_bytes);
            }
            bytes.resize(type_layout.layout.size as usize, 0);
            Ok(bytes)
        }
        ConstKind::Struct(fields) => {
            let struct_layout = type_layout.shape.as_struct().unwrap();
            let mut bytes = Vec::new();
            for (field, field_layout) in fields.iter().zip(struct_layout.fields.iter()) {
                assert!(field_layout.offset as usize >= bytes.len());
                bytes.resize(field_layout.offset as usize, 0);
                let mut field_bytes = lower_const_data(module, *field)?;
                bytes.append(&mut field_bytes);
            }
            bytes.resize(type_layout.layout.size as usize, 0);
            Ok(bytes)
        }
        ConstKind::String(string) => {
            let mut bytes = string.as_bytes().to_vec();
            assert!(
                bytes.len() <= type_layout.layout.size as usize,
                "String constant is {} bytes, but type layout size is {}",
                bytes.len(),
                type_layout.layout.size
            );
            bytes.resize(type_layout.layout.size as usize, 0);
            Ok(bytes)
        }
        ConstKind::Null => Ok(vec![0; type_layout.layout.size as usize]),
        ConstKind::Undef => todo!(),
    }
}

fn empty_machine_function<T: TargetArch>(name: String) -> Box<MachineFunction<T>> {
    Box::new(MachineFunction {
        name,
        blocks: Vec::new(),
        vreg_counter: VRegCounter(0),
        entry: BlockId(0),
        frame_info: FrameInfo {
            stack_slots: Vec::new(),
            max_align: 1,
            max_outgoing_arg_size: 0,
            used_callee_saved: HashSet::new(),
            need_save_ra: false,
        },
        frame_layout: FrameLayout {
            frame_size: 0,
            slot_offsets: HashMap::new(),
            outgoing_arg_offset: 0,
            incoming_arg_offset: 0,
            callee_saved_slots: HashMap::new(),
            ra_slot: None,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::{empty_machine_function, resolve_parallel_copy};
    use crate::mir::lower::CopyMove;
    use crate::mir::rv32im::{RV32Arch, RV32Inst};
    use crate::mir::{Register, VRegId};
    use std::collections::HashMap;

    fn run_virtual_moves(
        insts: &[RV32Inst],
        initial_values: &[(VRegId, i32)],
    ) -> HashMap<VRegId, i32> {
        let mut values: HashMap<VRegId, i32> = initial_values.iter().copied().collect();
        for inst in insts {
            match inst {
                RV32Inst::Mv {
                    rd: Register::Virtual(dst),
                    rs: Register::Virtual(src),
                } => {
                    let value = values[src];
                    values.insert(*dst, value);
                }
                other => panic!("unexpected instruction in parallel copy test: {other:?}"),
            }
        }
        values
    }

    #[test]
    fn resolve_parallel_copy_handles_acyclic_chain() {
        let mut machine_function = empty_machine_function("test".to_string());
        machine_function.vreg_counter.0 = 3;

        let insts = resolve_parallel_copy::<RV32Arch>(
            vec![
                CopyMove {
                    dst: Register::Virtual(VRegId(0)),
                    src: Register::Virtual(VRegId(1)),
                },
                CopyMove {
                    dst: Register::Virtual(VRegId(1)),
                    src: Register::Virtual(VRegId(2)),
                },
            ],
            &mut machine_function,
        );

        assert_eq!(machine_function.vreg_counter.0, 3);
        let values =
            run_virtual_moves(&insts, &[(VRegId(0), 10), (VRegId(1), 20), (VRegId(2), 30)]);
        assert_eq!(values[&VRegId(0)], 20);
        assert_eq!(values[&VRegId(1)], 30);
    }

    #[test]
    fn resolve_parallel_copy_handles_swap_cycle() {
        let mut machine_function = empty_machine_function("test".to_string());
        machine_function.vreg_counter.0 = 2;

        let insts = resolve_parallel_copy::<RV32Arch>(
            vec![
                CopyMove {
                    dst: Register::Virtual(VRegId(0)),
                    src: Register::Virtual(VRegId(1)),
                },
                CopyMove {
                    dst: Register::Virtual(VRegId(1)),
                    src: Register::Virtual(VRegId(0)),
                },
            ],
            &mut machine_function,
        );

        assert_eq!(machine_function.vreg_counter.0, 3);
        assert_eq!(insts.len(), 3);
        let values = run_virtual_moves(&insts, &[(VRegId(0), 10), (VRegId(1), 20)]);
        assert_eq!(values[&VRegId(0)], 20);
        assert_eq!(values[&VRegId(1)], 10);
    }

    #[test]
    fn resolve_parallel_copy_handles_fanout() {
        let mut machine_function = empty_machine_function("test".to_string());
        machine_function.vreg_counter.0 = 3;

        let insts = resolve_parallel_copy::<RV32Arch>(
            vec![
                CopyMove {
                    dst: Register::Virtual(VRegId(0)),
                    src: Register::Virtual(VRegId(2)),
                },
                CopyMove {
                    dst: Register::Virtual(VRegId(1)),
                    src: Register::Virtual(VRegId(2)),
                },
            ],
            &mut machine_function,
        );

        assert_eq!(machine_function.vreg_counter.0, 3);
        let values = run_virtual_moves(&insts, &[(VRegId(0), 1), (VRegId(1), 2), (VRegId(2), 99)]);
        assert_eq!(values[&VRegId(0)], 99);
        assert_eq!(values[&VRegId(1)], 99);
    }
}
