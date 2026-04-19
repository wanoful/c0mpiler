pub(crate) mod layout;
pub(crate) mod logue;
pub(crate) mod phi;
pub(crate) mod regalloc;

use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fmt::{self, Display, Formatter},
    marker::PhantomData,
    rc::Rc,
};

use crate::{
    ir::{
        LLVMModule,
        globalxxx::{FunctionPtr, GlobalVariablePtr},
        ir_value::{
            BasicBlockPtr, Constant, ConstantArray, ConstantInt, ConstantPtr, ConstantString,
            ConstantStruct, ICmpCode, InstructionPtr, Value, ValueKind, ValuePtr,
        },
        layout::LayoutShape,
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
        }
    }
}

impl Error for LowerError {}

fn lower_operand<R: TargetArch>(
    operand: &ValuePtr,
    out: &mut Vec<R::MachineInst>,
    state: &FunctionLoweringState,
    machine_function: &mut MachineFunction<R>,
) -> Result<Register<R::PhysicalReg>, LowerError> {
    if let ValueKind::Constant(Constant::ConstantInt(number)) = &operand.kind {
        let vreg = machine_function.new_vreg();
        out.push(R::MachineInst::load_imm(
            Register::Virtual(vreg),
            number.0 as i32,
        ));
        return Ok(Register::Virtual(vreg));
    }

    if let Some(vreg) = state.vreg_for_value(operand) {
        Ok(Register::Virtual(vreg))
    } else {
        Err(LowerError::UnknownBlock {
            function: state.function_name.clone(),
            block_name: operand.get_name(),
        })
    }
}

fn resolve_parallel_copy<R: TargetArch>(
    moves: Vec<(Register<R::PhysicalReg>, Register<R::PhysicalReg>)>,
    machine_function: &mut MachineFunction<R>,
) -> Vec<R::MachineInst> {
    let mut edges: HashMap<Register<R::PhysicalReg>, Vec<Register<R::PhysicalReg>>> =
        HashMap::new();
    for (dst, src) in moves {
        if dst != src {
            edges.entry(src).or_default().push(dst);
        }
    }

    let mut visited = HashSet::new();
    let mut visiting = HashSet::new();

    let mut middle_insts: Vec<R::MachineInst> = Vec::new();
    let mut first_insts: Vec<R::MachineInst> = Vec::new();
    let mut last_insts: Vec<R::MachineInst> = Vec::new();

    fn visit_fn<R: TargetArch>(
        node: Register<R::PhysicalReg>,
        visited: &mut HashSet<Register<R::PhysicalReg>>,
        visiting: &mut HashSet<Register<R::PhysicalReg>>,
        edges: &HashMap<Register<R::PhysicalReg>, Vec<Register<R::PhysicalReg>>>,
        middle_insts: &mut Vec<R::MachineInst>,
        first_insts: &mut Vec<R::MachineInst>,
        last_insts: &mut Vec<R::MachineInst>,
        machine_function: &mut MachineFunction<R>,
    ) {
        if visited.contains(&node) {
            return;
        }
        visiting.insert(node);
        visited.insert(node);

        if let Some(neighbors) = edges.get(&node) {
            for &neighbor in neighbors {
                visit_fn(
                    neighbor,
                    visited,
                    visiting,
                    edges,
                    middle_insts,
                    first_insts,
                    last_insts,
                    machine_function,
                );
                if visiting.contains(&neighbor) {
                    let temp = Register::Virtual(machine_function.new_vreg());
                    first_insts.push(R::MachineInst::mv(temp, node));
                    last_insts.push(R::MachineInst::mv(neighbor, temp));
                } else {
                    middle_insts.push(R::MachineInst::mv(neighbor, node));
                }
            }
        }

        visiting.remove(&node);
    }

    let nodes: Vec<_> = edges.keys().copied().collect();
    for node in nodes {
        visit_fn(
            node,
            &mut visited,
            &mut visiting,
            &edges,
            &mut middle_insts,
            &mut first_insts,
            &mut last_insts,
            machine_function,
        );
    }

    let mut out = Vec::new();
    out.extend(first_insts);
    out.extend(middle_insts);
    out.extend(last_insts);
    out
}

fn parallel_copy<R: TargetArch>(
    phis: Vec<(VRegId, ValuePtr)>,
    out: &mut Vec<R::MachineInst>,
    state: &mut FunctionLoweringState,
    machine_function: &mut MachineFunction<R>,
) -> Result<(), LowerError> {
    let mut moves = Vec::with_capacity(phis.len());
    for (dst, value) in phis {
        let src = lower_operand(&value, out, state, machine_function)?;
        moves.push((Register::Virtual(dst), src));
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
    base: Register<T::PhysicalReg>,
    stride: u32,
    index: &ValuePtr,
    out: &mut Vec<T::MachineInst>,
    machine_function: &mut MachineFunction<T>,
    state: &mut FunctionLoweringState,
) -> Result<Register<T::PhysicalReg>, LowerError> {
    if let ValueKind::Constant(Constant::ConstantInt(number)) = &index.kind {
        let imm = (number.0 as i32).checked_mul(stride as i32).unwrap();
        if (-2048..=2047).contains(&imm) {
            let result_reg = Register::Virtual(machine_function.new_vreg());
            out.push(T::emit_addi(result_reg, base, imm));
            Ok(result_reg)
        } else {
            let temp_reg = Register::Virtual(machine_function.new_vreg());
            out.push(T::MachineInst::load_imm(temp_reg, imm));
            let result_reg = Register::Virtual(machine_function.new_vreg());
            out.push(T::emit_add(result_reg, base, temp_reg));
            Ok(result_reg)
        }
    } else {
        if stride == 0 {
            return Ok(base);
        }

        let index_reg = lower_operand(index, out, state, machine_function)?;
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
    block_map: HashMap<*const Value, BlockId>,
    block_order: Vec<BasicBlockPtr>,
    value_vregs: HashMap<*const Value, VRegId>,
    stack_slots: HashMap<*const Value, StackSlotId>,
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

    fn record_block(&mut self, block: &BasicBlockPtr, id: BlockId) {
        self.block_map.insert(Rc::as_ptr(block), id);
        self.block_order.push(block.clone());
    }

    fn block_id(&self, block: &ValuePtr) -> Option<BlockId> {
        self.block_map.get(&Rc::as_ptr(block)).copied()
    }

    fn record_vreg(&mut self, value: &ValuePtr, vreg: VRegId) {
        self.value_vregs.insert(Rc::as_ptr(value), vreg);
    }

    fn vreg_for_value(&self, value: &ValuePtr) -> Option<VRegId> {
        self.value_vregs.get(&Rc::as_ptr(value)).copied()
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

    pub fn lower_module(&mut self, module: &LLVMModule) -> Result<MachineModule<T>, LowerError> {
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
        module: &LLVMModule,
        machine_module: &mut MachineModule<T>,
        state: &mut ModuleLoweringState,
    ) -> Result<(), LowerError> {
        for global in module.global_variables() {
            let name = global
                .get_name()
                .expect("global variables should always have symbol names");
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
            let name = function.get_name().ok_or(LowerError::MissingFunctionName)?;
            self.ensure_symbol_absent(machine_module, &name)?;

            let is_external = function.as_function().blocks.borrow().is_empty();
            let linkage = if is_external {
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
        module: &LLVMModule,
        machine_module: &mut MachineModule<T>,
        state: &ModuleLoweringState,
    ) -> Result<(), LowerError> {
        for global in module.global_variables() {
            let name = global
                .get_name()
                .expect("global variables should always have symbol names");
            let symbol_id = state.global_symbols[&name];
            let symbol = &mut machine_module.symbols[symbol_id.0];
            symbol.kind = self.lower_global(module, &global)?;
            if global.as_global_variable().is_constant {
                symbol.segment = MachineSegment::ReadOnlyData;
            }
            symbol.alignment = module
                .get_type_layout(global.get_type())
                .unwrap()
                .layout
                .align as usize;
        }

        Ok(())
    }

    fn lower_functions(
        &mut self,
        module: &LLVMModule,
        machine_module: &mut MachineModule<T>,
        state: &ModuleLoweringState,
    ) -> Result<(), LowerError> {
        for function in module.functions_in_order() {
            if function.as_function().blocks.borrow().is_empty() {
                continue;
            }

            let name = function.get_name().unwrap();
            let symbol_id = state.function_symbols[&name];
            let lowered = self.lower_function(&function, module, machine_module)?;
            machine_module.symbols[symbol_id.0].kind = MachineSymbolKind::Function(lowered);
        }

        Ok(())
    }

    fn lower_global(
        &mut self,
        module: &LLVMModule,
        global: &GlobalVariablePtr,
    ) -> Result<MachineSymbolKind<T>, LowerError> {
        let initializer = global.as_global_variable().initializer.clone();

        Ok(MachineSymbolKind::Data(
            self.lower_constant(module, &initializer)?,
        ))
    }

    fn lower_constant(
        &self,
        module: &LLVMModule,
        constant: &ConstantPtr,
    ) -> Result<Vec<u8>, LowerError> {
        let ty = constant.get_type();
        let type_layout = module.get_type_layout(ty).unwrap();

        match constant.as_constant() {
            Constant::ConstantInt(ConstantInt(number)) => {
                let bytes = number.to_le_bytes();
                Ok(bytes[..(type_layout.layout.size as usize)].to_vec())
            }
            Constant::ConstantArray(ConstantArray(inners)) => {
                let array_layout = type_layout.shape.as_array().unwrap();
                inners.iter().try_fold(vec![], |mut acc, inner| {
                    let mut inner_bytes = self.lower_constant(module, inner)?;
                    assert!(inner_bytes.len() <= array_layout.stride as usize);
                    inner_bytes.resize(array_layout.stride as usize, 0);
                    acc.append(&mut inner_bytes);
                    Ok(acc)
                })
            }
            Constant::ConstantStruct(ConstantStruct(fields)) => {
                let struct_layout = type_layout.shape.as_struct().unwrap();
                fields.iter().zip(struct_layout.fields.iter()).try_fold(
                    vec![],
                    |mut acc, (field, field_layout)| {
                        assert!(field_layout.offset as usize >= acc.len());
                        acc.resize(field_layout.offset as usize, 0);
                        let mut field_bytes = self.lower_constant(module, field)?;
                        acc.append(&mut field_bytes);
                        Ok(acc)
                    },
                )
            }
            Constant::ConstantString(ConstantString(string)) => {
                let mut bytes = string.as_bytes().to_vec();
                bytes.push(0);
                assert!(bytes.len() <= type_layout.layout.size as usize);
                bytes.resize(type_layout.layout.size as usize, 0);
                Ok(bytes)
            }
            Constant::ConstantNull(..) => Err(LowerError::UnimplementedGlobal(
                "Lowering null constant is not implemented yet".to_string(),
            )),
        }
    }

    fn lower_function(
        &mut self,
        function: &FunctionPtr,
        module: &LLVMModule,
        machine_module: &mut MachineModule<T>,
    ) -> Result<MachineFunction<T>, LowerError> {
        let function_name = function.get_name().unwrap();
        let blocks = function.as_function().blocks.borrow().clone();
        let entry = blocks
            .first()
            .cloned()
            .ok_or_else(|| LowerError::MissingEntryBlock {
                function: function_name.clone(),
            })?;

        let mut state = FunctionLoweringState::new(function_name.clone());
        let mut machine_function = empty_machine_function(function_name.clone());

        machine_function.entry = BlockId(0);
        self.initialize_blocks(&blocks, &mut machine_function, &mut state)?;
        self.collect_phis(&mut machine_function, &mut state)?;
        self.initialize_func_arguments(function, &mut machine_function, &mut state);

        machine_function.entry =
            state
                .block_id(&entry)
                .ok_or_else(|| LowerError::UnknownBlock {
                    function: function_name.clone(),
                    block_name: entry.get_name(),
                })?;

        for block in &blocks {
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

        Ok(machine_function)
    }

    fn initialize_blocks(
        &mut self,
        blocks: &[BasicBlockPtr],
        machine_function: &mut MachineFunction<T>,
        state: &mut FunctionLoweringState,
    ) -> Result<(), LowerError> {
        for (index, block) in blocks.iter().enumerate() {
            let id = BlockId(index);
            let name = block.get_name().unwrap_or_else(|| format!(".bb{index}"));
            machine_function.blocks.push(MachineBlock {
                id,
                name,
                instructions: Vec::new(),
            });
            state.record_block(block, id);
        }

        Ok(())
    }

    fn lower_block(
        &mut self,
        block: &BasicBlockPtr,
        machine_function: &mut MachineFunction<T>,
        module: &LLVMModule,
        machine_module: &mut MachineModule<T>,
        state: &mut FunctionLoweringState,
    ) -> Result<(), LowerError> {
        let block_id = state.block_id(block).unwrap();
        let instructions = block.as_basic_block().instructions.borrow();

        for instruction in instructions.iter() {
            let insts = self.lower_instruction(
                instruction,
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
        instruction: &InstructionPtr,
        block_id: BlockId,
        module: &LLVMModule,
        machine_function: &mut MachineFunction<T>,
        machine_module: &mut MachineModule<T>,
        state: &mut FunctionLoweringState,
    ) -> Result<Vec<T::MachineInst>, LowerError> {
        let inst = instruction.as_instruction();
        let operands = &inst.operands;

        let mut out = Vec::new();

        use crate::ir::ir_value::InstructionKind::*;
        match &inst.kind {
            Binary(binary_opcode) => {
                use crate::ir::ir_value::BinaryOpcode::*;

                let rs1 = lower_operand(&operands[0], &mut out, state, machine_function)?;

                if let crate::ir::ir_value::ValueKind::Constant(Constant::ConstantInt(number)) =
                    &operands[1].kind
                    && matches!(
                        binary_opcode,
                        Add | Sub | Shl | LShr | AShr | And | Or | Xor
                    )
                {
                    let rd_vreg = machine_function.new_vreg();
                    let rd = Register::Virtual(rd_vreg);
                    let imm = number.0 as i32;
                    let inst = match binary_opcode {
                        Add if (-2048..=2047).contains(&imm) => Some(T::emit_addi(rd, rs1, imm)),
                        Sub => imm
                            .checked_neg()
                            .filter(|neg_imm| (-2048..=2047).contains(neg_imm))
                            .map(|neg_imm| T::emit_addi(rd, rs1, neg_imm)),
                        Shl if (0..32).contains(&imm) => Some(T::emit_slli(rd, rs1, imm)),
                        LShr if (0..32).contains(&imm) => Some(T::emit_srli(rd, rs1, imm)),
                        AShr if (0..32).contains(&imm) => Some(T::emit_srai(rd, rs1, imm)),
                        And if (-2048..=2047).contains(&imm) => Some(T::emit_andi(rd, rs1, imm)),
                        Or if (-2048..=2047).contains(&imm) => Some(T::emit_ori(rd, rs1, imm)),
                        Xor if (-2048..=2047).contains(&imm) => Some(T::emit_xori(rd, rs1, imm)),
                        _ => None,
                    };

                    if let Some(inst) = inst {
                        out.push(inst);
                        state.record_vreg(instruction, rd_vreg);
                        return Ok(out);
                    }
                }

                let rs2 = lower_operand(&operands[1], &mut out, state, machine_function)?;
                let rd_vreg = machine_function.new_vreg();
                let rd = Register::Virtual(rd_vreg);

                let inst = match binary_opcode {
                    Add => T::emit_add(rd, rs1, rs2),
                    Sub => T::emit_sub(rd, rs1, rs2),
                    Mul => T::emit_mul(rd, rs1, rs2),
                    UDiv => T::emit_divu(rd, rs1, rs2),
                    SDiv => T::emit_div(rd, rs1, rs2),
                    URem => T::emit_remu(rd, rs1, rs2),
                    SRem => T::emit_rem(rd, rs1, rs2),
                    And => T::emit_and(rd, rs1, rs2),
                    Or => T::emit_or(rd, rs1, rs2),
                    Xor => T::emit_xor(rd, rs1, rs2),
                    Shl => T::emit_sll(rd, rs1, rs2),
                    LShr => T::emit_srl(rd, rs1, rs2),
                    AShr => T::emit_sra(rd, rs1, rs2),
                };

                out.push(inst);
                state.record_vreg(instruction, rd_vreg);
            }
            Call => {
                let func_id = machine_module.symbol_map[&operands[0].get_name().unwrap()];
                let args = &operands[1..];
                let num_args = args.len();
                let stack_arg_size =
                    num_args.saturating_sub(T::num_arg_regs()) * T::stack_arg_size();
                machine_function.record_outgoing_arg(stack_arg_size);

                for (index, arg) in args.iter().enumerate() {
                    let rs = lower_operand(arg, &mut out, state, machine_function)?;
                    if index < T::num_arg_regs() {
                        out.push(T::MachineInst::mv(
                            Register::Physical(T::arg_reg(index)),
                            rs,
                        ));
                    } else {
                        out.push(T::emit_store_outgoing_arg(
                            rs,
                            T::stack_arg_offset(index - T::num_arg_regs()),
                        ));
                    }
                }

                out.push(T::emit_call(func_id, num_args));

                if !instruction.get_type().is_void() {
                    let rd_vreg = machine_function.new_vreg();
                    out.push(T::MachineInst::mv(
                        Register::Virtual(rd_vreg),
                        Register::Physical(T::return_reg()),
                    ));
                    state.record_vreg(instruction, rd_vreg);
                }
            }
            Branch { has_cond } => {
                let collect_phis_from_info = |inner: &Vec<PhiInfo>| {
                    inner
                        .iter()
                        .filter_map(|info| {
                            info.filter_pred(block_id)
                                .map(|ptr| (info.get_dst(), ptr.clone()))
                        })
                        .collect()
                };

                if *has_cond {
                    let cond = lower_operand(&operands[0], &mut out, state, machine_function)?;
                    let mut true_block_id = state
                        .block_id(&BasicBlockPtr(operands[1].clone()))
                        .ok_or_else(|| LowerError::UnknownBlock {
                            function: state.function_name.clone(),
                            block_name: operands[1].get_name(),
                        })?;
                    let mut false_block_id = state
                        .block_id(&BasicBlockPtr(operands[2].clone()))
                        .ok_or_else(|| LowerError::UnknownBlock {
                            function: state.function_name.clone(),
                            block_name: operands[2].get_name(),
                        })?;

                    let true_block_phis: Option<Vec<_>> = state
                        .phi_infos
                        .get(&true_block_id)
                        .map(collect_phis_from_info);
                    let false_block_phis: Option<Vec<_>> = state
                        .phi_infos
                        .get(&false_block_id)
                        .map(collect_phis_from_info);

                    let mut add_transit_block =
                        |target_block_id: BlockId, target_block_phis: Vec<(VRegId, ValuePtr)>| {
                            let transit_block_id = machine_function.blocks.len();
                            let mut transit_insts: Vec<T::MachineInst> = Vec::new();

                            parallel_copy(
                                target_block_phis,
                                &mut transit_insts,
                                state,
                                machine_function,
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
                    let target_block_id = state
                        .block_id(&BasicBlockPtr(operands[0].clone()))
                        .ok_or_else(|| LowerError::UnknownBlock {
                            function: state.function_name.clone(),
                            block_name: operands[0].get_name(),
                        })?;

                    let target_block_phis: Option<Vec<_>> = state
                        .phi_infos
                        .get(&target_block_id)
                        .map(collect_phis_from_info);

                    if let Some(target_block_phis) = target_block_phis
                        && !target_block_phis.is_empty()
                    {
                        parallel_copy(target_block_phis, &mut out, state, machine_function)?;
                    }

                    out.push(T::emit_jump(target_block_id));
                }
            }
            GetElementPtr { base_ty } => {
                let mut base = lower_operand(&operands[0], &mut out, state, machine_function)?;
                let base_index = &operands[1];
                let indices = &operands[2..];

                let mut ty = base_ty.clone();
                let type_layout = module.get_type_layout(&ty).unwrap();
                base = emit_add_offset(
                    base,
                    type_layout.layout.stride(),
                    base_index,
                    &mut out,
                    machine_function,
                    state,
                )?;

                for index in indices {
                    let type_layout = module.get_type_layout(&ty).unwrap();
                    match type_layout.shape {
                        LayoutShape::Struct(_) => {
                            let struct_layout = type_layout.shape.as_struct().unwrap();
                            let field_index =
                                if let ValueKind::Constant(Constant::ConstantInt(number)) =
                                    &index.kind
                                {
                                    number.0 as usize
                                } else {
                                    return Err(LowerError::UnimplementedInstruction {
                                        function: state.function_name.clone(),
                                        opcode: "non-constant struct GEP index".to_string(),
                                    });
                                };
                            let field_layout = &struct_layout.fields[field_index];
                            let temp_reg = Register::Virtual(machine_function.new_vreg());
                            if field_layout.offset <= 2047 {
                                out.push(T::emit_addi(temp_reg, base, field_layout.offset as i32));
                            } else {
                                let imm_reg = Register::Virtual(machine_function.new_vreg());
                                out.push(T::MachineInst::load_imm(
                                    imm_reg,
                                    field_layout.offset as i32,
                                ));
                                out.push(T::emit_add(temp_reg, base, imm_reg));
                            }
                            base = temp_reg;
                            ty = ty.as_struct().unwrap().get_body().unwrap()[field_index].clone();
                        }
                        LayoutShape::Array(_) => {
                            let array_layout = type_layout.shape.as_array().unwrap();
                            base = emit_add_offset(
                                base,
                                array_layout.stride,
                                index,
                                &mut out,
                                machine_function,
                                state,
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

                let result_vreg = match base {
                    Register::Virtual(vreg) => vreg,
                    Register::Physical(_) => {
                        let result_vreg = machine_function.new_vreg();
                        out.push(T::MachineInst::mv(Register::Virtual(result_vreg), base));
                        result_vreg
                    }
                };
                state.record_vreg(instruction, result_vreg);
            }
            Alloca { inner_ty } => {
                let stack_slot_id = machine_function.new_stack_slot(
                    module.get_type_layout(inner_ty).unwrap().layout.size as usize,
                    module.get_type_layout(inner_ty).unwrap().layout.align as usize,
                    crate::mir::StackSlotKind::Alloca,
                );
                state
                    .stack_slots
                    .insert(Rc::as_ptr(instruction), stack_slot_id);
                let rd_vreg = machine_function.new_vreg();
                out.push(T::emit_get_stack_addr(
                    Register::Virtual(rd_vreg),
                    stack_slot_id,
                ));
                state.record_vreg(instruction, rd_vreg);
            }
            Load => {
                let rd_vreg = machine_function.new_vreg();
                let ty = instruction.get_type();
                let type_layout = module.get_type_layout(ty).unwrap();
                let rd = Register::Virtual(rd_vreg);
                let (symbol_id, rs1) = if operands[0].kind.is_global_object() {
                    let name = operands[0].get_name().unwrap();
                    let symbol_id = machine_module.symbol_map[&name];
                    (Some(symbol_id), Register::Physical(T::zero_reg()))
                } else {
                    (
                        None,
                        lower_operand(&operands[0], &mut out, state, machine_function)?,
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
                state.record_vreg(instruction, rd_vreg);
            }
            Ret { is_void } => {
                if !*is_void {
                    let rs = lower_operand(&operands[0], &mut out, state, machine_function)?;
                    out.push(T::MachineInst::mv(Register::Physical(T::return_reg()), rs));
                }
                out.push(T::emit_ret());
            }
            Store => {
                let rs2 = lower_operand(&operands[0], &mut out, state, machine_function)?;
                let addr = &operands[1];
                let type_layout = module.get_type_layout(operands[0].get_type()).unwrap();
                if addr.kind.is_global_object() {
                    let name = addr.get_name().unwrap();
                    let symbol = machine_module.symbol_map[&name];
                    match type_layout.layout.size as usize {
                        1 | 2 | 4 => {
                            out.push(T::emit_store_global(
                                rs2,
                                symbol,
                                type_layout.layout.size as usize,
                            ));
                        }
                        _ => panic!("unsupported store size"),
                    }
                } else {
                    let rs1 = lower_operand(addr, &mut out, state, machine_function)?;
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
            Icmp(icmp_code) => {
                let rs1 = lower_operand(&operands[0], &mut out, state, machine_function)?;
                let rs2 = lower_operand(&operands[1], &mut out, state, machine_function)?;
                let rd_vreg = machine_function.new_vreg();
                let rd = Register::Virtual(rd_vreg);
                emit_icmp(icmp_code, rd, rs1, rs2, &mut out, machine_function);
                state.record_vreg(instruction, rd_vreg);
            }
            Phi => {
                assert!(
                    state.vreg_for_value(instruction).is_some(),
                    "phi node should have been assigned a vreg in the first pass"
                );
            }
            Select => {
                let cond = lower_operand(&operands[0], &mut out, state, machine_function)?;
                let true_val = lower_operand(&operands[1], &mut out, state, machine_function)?;
                let false_val = lower_operand(&operands[2], &mut out, state, machine_function)?;

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
                let rd_vreg = machine_function.new_vreg();
                out.push(T::emit_or(
                    Register::Virtual(rd_vreg),
                    Register::Virtual(true_part),
                    Register::Virtual(false_part),
                ));
                state.record_vreg(instruction, rd_vreg);
            }
            PtrToInt => {
                let src = lower_operand(&operands[0], &mut out, state, machine_function)?;
                let rd_vreg = machine_function.new_vreg();
                out.push(T::MachineInst::mv(Register::Virtual(rd_vreg), src));
                state.record_vreg(instruction, rd_vreg);
            }
            Trunc => {
                let dst_ty = instruction.get_type();
                let src = lower_operand(&operands[0], &mut out, state, machine_function)?;
                let dst_bits = dst_ty.as_int().unwrap().0;
                let masked = emit_masked_value(src, dst_bits, &mut out, machine_function);
                let rd_vreg = match masked {
                    Register::Virtual(vreg) => vreg,
                    Register::Physical(_) => unreachable!("masked value should be virtual"),
                };
                state.record_vreg(instruction, rd_vreg);
            }
            Zext => {
                let src = lower_operand(&operands[0], &mut out, state, machine_function)?;
                let src_bits = operands[0].get_type().as_int().unwrap().0;
                let extended = emit_masked_value(src, src_bits, &mut out, machine_function);
                let rd_vreg = match extended {
                    Register::Virtual(vreg) => vreg,
                    Register::Physical(_) => unreachable!("zero-extended value should be virtual"),
                };
                state.record_vreg(instruction, rd_vreg);
            }
            Sext => {
                let src = lower_operand(&operands[0], &mut out, state, machine_function)?;
                let rd_vreg = machine_function.new_vreg();
                let src_bits = operands[0].get_type().as_int().unwrap().0;
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
                state.record_vreg(instruction, rd_vreg);
            }
            Unreachable => {}
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
        function: &FunctionPtr,
        machine_function: &mut MachineFunction<T>,
        state: &mut FunctionLoweringState,
    ) {
        let params = &function.as_function().params;

        let mut insts = params
            .iter()
            .enumerate()
            .map(|(index, param)| {
                let vreg = machine_function.new_vreg();
                state.record_vreg(param, vreg);
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
        let mut preds: HashMap<BlockId, HashSet<BlockId>> = HashMap::new();
        let mut succs: HashMap<BlockId, HashSet<BlockId>> = HashMap::new();

        for block in machine_function.blocks.iter() {
            preds.entry(block.id).or_default();
            succs.entry(block.id).or_default();
        }

        for block in machine_function.blocks.iter() {
            for inst in block.instructions.iter().rev() {
                if !inst.is_terminator() {
                    break;
                }
                inst.get_successors().into_iter().for_each(|succ| {
                    preds.entry(succ).or_default().insert(block.id);
                    succs.entry(block.id).or_default().insert(succ);
                });
            }
        }

        ControlFlowGraph { succs, preds }
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

fn empty_machine_function<T: TargetArch>(name: String) -> MachineFunction<T> {
    MachineFunction {
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
    }
}

#[cfg(test)]
mod tests {
    use super::{empty_machine_function, resolve_parallel_copy};
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
                (Register::Virtual(VRegId(0)), Register::Virtual(VRegId(1))),
                (Register::Virtual(VRegId(1)), Register::Virtual(VRegId(2))),
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
                (Register::Virtual(VRegId(0)), Register::Virtual(VRegId(1))),
                (Register::Virtual(VRegId(1)), Register::Virtual(VRegId(0))),
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
                (Register::Virtual(VRegId(0)), Register::Virtual(VRegId(2))),
                (Register::Virtual(VRegId(1)), Register::Virtual(VRegId(2))),
            ],
            &mut machine_function,
        );

        assert_eq!(machine_function.vreg_counter.0, 3);
        let values = run_virtual_moves(&insts, &[(VRegId(0), 1), (VRegId(1), 2), (VRegId(2), 99)]);
        assert_eq!(values[&VRegId(0)], 99);
        assert_eq!(values[&VRegId(1)], 99);
    }
}
