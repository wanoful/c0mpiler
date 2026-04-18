pub mod phi;

use std::{
    collections::HashMap,
    error::Error,
    fmt::{self, Display, Formatter},
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
        BlockId, FrameInfo, FrameLayout, Linkage, MachineBlock, MachineFunction, MachineModule,
        MachineSegment, MachineSymbolKind, Register, StackSlotId, SymbolId, VRegId,
        lower::phi::PhiInfo,
        rv32im::{RV32Arch, RV32Inst, RV32Reg},
    },
};

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

#[derive(Debug, Default)]
struct ModuleLoweringState {
    function_symbols: HashMap<String, SymbolId>,
    global_symbols: HashMap<String, SymbolId>,
}

#[derive(Debug)]
struct FunctionLoweringState {
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

#[derive(Debug, Default)]
pub struct RV32Lowerer {
    options: LowerOptions,
}

impl RV32Lowerer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_options(options: LowerOptions) -> Self {
        Self { options }
    }

    pub fn lower_module(
        &mut self,
        module: &LLVMModule,
    ) -> Result<MachineModule<RV32Arch>, LowerError> {
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
        machine_module: &mut MachineModule<RV32Arch>,
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
        machine_module: &mut MachineModule<RV32Arch>,
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
        machine_module: &mut MachineModule<RV32Arch>,
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
    ) -> Result<MachineSymbolKind<RV32Arch>, LowerError> {
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
        machine_module: &mut MachineModule<RV32Arch>,
    ) -> Result<MachineFunction<RV32Arch>, LowerError> {
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

        Ok(machine_function)
    }

    fn initialize_blocks(
        &mut self,
        blocks: &[BasicBlockPtr],
        machine_function: &mut MachineFunction<RV32Arch>,
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
        machine_function: &mut MachineFunction<RV32Arch>,
        module: &LLVMModule,
        machine_module: &mut MachineModule<RV32Arch>,
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
        machine_function: &mut MachineFunction<RV32Arch>,
        machine_module: &mut MachineModule<RV32Arch>,
        state: &mut FunctionLoweringState,
    ) -> Result<Vec<RV32Inst>, LowerError> {
        let inst = instruction.as_instruction();
        let operands = &inst.operands;

        fn lower_operand(
            operand: &ValuePtr,
            out: &mut Vec<RV32Inst>,
            state: &FunctionLoweringState,
            machine_function: &mut MachineFunction<RV32Arch>,
        ) -> Result<Register<RV32Reg>, LowerError> {
            if let ValueKind::Constant(Constant::ConstantInt(number)) = &operand.kind {
                let vreg = machine_function.new_vreg();
                out.push(RV32Inst::Li {
                    rd: Register::Virtual(vreg),
                    imm: number.0 as i32,
                });
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

        fn emit_masked_value(
            src: Register<RV32Reg>,
            bits: u8,
            out: &mut Vec<RV32Inst>,
            machine_function: &mut MachineFunction<RV32Arch>,
        ) -> Register<RV32Reg> {
            if bits >= 32 {
                return src;
            }

            let rd = Register::Virtual(machine_function.new_vreg());
            let mask = (1u32 << bits) - 1;
            if mask <= 0x7ff {
                out.push(RV32Inst::Andi {
                    rd,
                    rs1: src,
                    imm: mask as i32,
                });
            } else {
                let mask_reg = Register::Virtual(machine_function.new_vreg());
                out.push(RV32Inst::Li {
                    rd: mask_reg,
                    imm: mask as i32,
                });
                out.push(RV32Inst::And {
                    rd,
                    rs1: src,
                    rs2: mask_reg,
                });
            }

            rd
        }

        let mut out = Vec::new();

        use crate::ir::ir_value::InstructionKind::*;
        match &inst.kind {
            Binary(binary_opcode) => {
                use crate::ir::ir_value::BinaryOpcode::*;

                let rs1 = lower_operand(&operands[0], &mut out, state, machine_function)?;

                if let crate::ir::ir_value::ValueKind::Constant(Constant::ConstantInt(number)) =
                    &operands[1].kind
                    && number.0 as i32 <= 2047
                    && number.0 as i32 >= -2048
                    && matches!(
                        binary_opcode,
                        Add | Sub | Shl | LShr | AShr | And | Or | Xor
                    )
                {
                    let rd_vreg = machine_function.new_vreg();
                    let rd = Register::Virtual(rd_vreg);
                    let imm = number.0 as i32;
                    let inst = match binary_opcode {
                        Add => Some(RV32Inst::Addi { rd, rs1, imm }),
                        Sub => Some(RV32Inst::Addi { rd, rs1, imm: -imm }),
                        Shl => Some(RV32Inst::Slli { rd, rs1, imm }),
                        LShr => Some(RV32Inst::Srli { rd, rs1, imm }),
                        AShr => Some(RV32Inst::Srai { rd, rs1, imm }),
                        And => Some(RV32Inst::Andi { rd, rs1, imm }),
                        Or => Some(RV32Inst::Ori { rd, rs1, imm }),
                        Xor => Some(RV32Inst::Xori { rd, rs1, imm }),
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
                    Add => RV32Inst::Add { rd, rs1, rs2 },
                    Sub => RV32Inst::Sub { rd, rs1, rs2 },
                    Mul => RV32Inst::Mul { rd, rs1, rs2 },
                    UDiv => RV32Inst::Divu { rd, rs1, rs2 },
                    SDiv => RV32Inst::Div { rd, rs1, rs2 },
                    URem => RV32Inst::Remu { rd, rs1, rs2 },
                    SRem => RV32Inst::Rem { rd, rs1, rs2 },
                    And => RV32Inst::And { rd, rs1, rs2 },
                    Or => RV32Inst::Or { rd, rs1, rs2 },
                    Xor => RV32Inst::Xor { rd, rs1, rs2 },
                    Shl => RV32Inst::Sll { rd, rs1, rs2 },
                    LShr => RV32Inst::Srl { rd, rs1, rs2 },
                    AShr => RV32Inst::Sra { rd, rs1, rs2 },
                };

                out.push(inst);
                state.record_vreg(instruction, rd_vreg);
            }
            Call => {
                let func_id = machine_module.symbol_map[&operands[0].get_name().unwrap()];
                let args = &operands[1..];
                let num_args = args.len();
                let stack_arg_size = if num_args > 8 { (num_args - 8) * 4 } else { 0 };
                machine_function.record_outgoing_arg(stack_arg_size);

                for (index, arg) in args.iter().enumerate() {
                    let rs = lower_operand(arg, &mut out, state, machine_function)?;
                    if index < 8 {
                        out.push(RV32Inst::Mv {
                            rd: Register::Physical(RV32Reg::reg_a(index)),
                            rs,
                        });
                    } else {
                        let offset = (index - 8) * 4;
                        out.push(RV32Inst::StoreOutgoingArg {
                            rs,
                            offset: offset as i32,
                        });
                    }
                }

                out.push(RV32Inst::Call {
                    func: func_id,
                    num_args,
                });

                if !instruction.get_type().is_void() {
                    let rd_vreg = machine_function.new_vreg();
                    out.push(RV32Inst::Mv {
                        rd: Register::Virtual(rd_vreg),
                        rs: Register::Physical(RV32Reg::A0),
                    });
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
                            let mut transit_insts: Vec<RV32Inst> = Vec::new();

                            for (dst, value) in target_block_phis.iter() {
                                let src = lower_operand(
                                    value,
                                    &mut transit_insts,
                                    state,
                                    machine_function,
                                )?;
                                transit_insts.push(RV32Inst::Mv {
                                    rd: Register::Virtual(*dst),
                                    rs: src,
                                });
                            }
                            transit_insts.push(RV32Inst::Jal {
                                rd: Register::Physical(RV32Reg::Zero),
                                label: target_block_id,
                            });
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

                    out.push(RV32Inst::Bne {
                        rs1: cond,
                        rs2: Register::Physical(RV32Reg::Zero),
                        label: true_block_id,
                    });
                    out.push(RV32Inst::Jal {
                        rd: Register::Physical(RV32Reg::Zero),
                        label: false_block_id,
                    });
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

                    if let Some(target_block_phis) = &target_block_phis {
                        for (dst, value) in target_block_phis {
                            let src = lower_operand(value, &mut out, state, machine_function)?;
                            out.push(RV32Inst::Mv {
                                rd: Register::Virtual(*dst),
                                rs: src,
                            });
                        }
                    };

                    out.push(RV32Inst::Jal {
                        rd: Register::Physical(RV32Reg::Zero),
                        label: target_block_id,
                    });
                }
            }
            GetElementPtr { base_ty } => {
                fn add_offset(
                    base: Register<RV32Reg>,
                    stride: u32,
                    index: &ValuePtr,
                    out: &mut Vec<RV32Inst>,
                    machine_function: &mut MachineFunction<RV32Arch>,
                    state: &mut FunctionLoweringState,
                ) -> Result<Register<RV32Reg>, LowerError> {
                    if let ValueKind::Constant(Constant::ConstantInt(number)) = &index.kind {
                        let imm = (number.0 as i32).checked_mul(stride as i32).unwrap();
                        if imm <= 2047 && imm >= -2048 {
                            let result_vreg = machine_function.new_vreg();
                            let result_reg = Register::Virtual(result_vreg);
                            out.push(RV32Inst::Addi {
                                rd: result_reg,
                                rs1: base,
                                imm,
                            });
                            Ok(result_reg)
                        } else {
                            let temp_vreg = machine_function.new_vreg();
                            let temp_reg = Register::Virtual(temp_vreg);
                            out.push(RV32Inst::Li { rd: temp_reg, imm });
                            let result_vreg = machine_function.new_vreg();
                            let result_reg = Register::Virtual(result_vreg);
                            out.push(RV32Inst::Add {
                                rd: result_reg,
                                rs1: base,
                                rs2: temp_reg,
                            });
                            Ok(result_reg)
                        }
                    } else {
                        if stride == 0 {
                            return Ok(base);
                        }

                        let index_reg = lower_operand(index, out, state, machine_function)?;
                        let temp_vreg = machine_function.new_vreg();
                        let temp_reg = Register::Virtual(temp_vreg);
                        if stride.is_power_of_two() {
                            out.push(RV32Inst::Slli {
                                rd: temp_reg,
                                rs1: index_reg,
                                imm: stride.trailing_zeros() as i32,
                            });
                        } else {
                            let stride_reg = Register::Virtual(machine_function.new_vreg());
                            out.push(RV32Inst::Li {
                                rd: stride_reg,
                                imm: stride as i32,
                            });
                            out.push(RV32Inst::Mul {
                                rd: temp_reg,
                                rs1: index_reg,
                                rs2: stride_reg,
                            });
                        }
                        let result_vreg = machine_function.new_vreg();
                        let result_reg = Register::Virtual(result_vreg);
                        out.push(RV32Inst::Add {
                            rd: result_reg,
                            rs1: base,
                            rs2: temp_reg,
                        });
                        Ok(result_reg)
                    }
                }

                let mut base = lower_operand(&operands[0], &mut out, state, machine_function)?;
                let base_index = &operands[1];
                let indices = &operands[2..];

                let mut ty = base_ty.clone();
                let type_layout = module.get_type_layout(&ty).unwrap();
                base = add_offset(
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
                            let temp_vreg = machine_function.new_vreg();
                            let temp_reg = Register::Virtual(temp_vreg);
                            if field_layout.offset <= 2047 {
                                out.push(RV32Inst::Addi {
                                    rd: temp_reg,
                                    rs1: base,
                                    imm: field_layout.offset as i32,
                                });
                            } else {
                                let imm_reg = Register::Virtual(machine_function.new_vreg());
                                out.push(RV32Inst::Li {
                                    rd: imm_reg,
                                    imm: field_layout.offset as i32,
                                });
                                out.push(RV32Inst::Add {
                                    rd: temp_reg,
                                    rs1: base,
                                    rs2: imm_reg,
                                });
                            }
                            base = temp_reg;
                            ty = ty.as_struct().unwrap().get_body().unwrap()[field_index].clone();
                        }
                        LayoutShape::Array(_) => {
                            let array_layout = type_layout.shape.as_array().unwrap();
                            base = add_offset(
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
                        out.push(RV32Inst::Mv {
                            rd: Register::Virtual(result_vreg),
                            rs: base,
                        });
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
                out.push(RV32Inst::GetStackAddr {
                    rd: Register::Virtual(rd_vreg),
                    slot: stack_slot_id,
                });
                state.record_vreg(instruction, rd_vreg);
            }
            Load => {
                let rd_vreg = machine_function.new_vreg();
                let ty = instruction.get_type();
                let type_layout = module.get_type_layout(ty).unwrap();
                let (symbol_id, rs1) = if operands[0].kind.is_global_object() {
                    let name = operands[0].get_name().unwrap();
                    let symbol_id = machine_module.symbol_map[&name];
                    (Some(symbol_id), Register::Physical(RV32Reg::Zero))
                } else {
                    (
                        None,
                        lower_operand(&operands[0], &mut out, state, machine_function)?,
                    )
                };

                match type_layout.layout.size {
                    1 => {
                        if let Some(symbol_id) = symbol_id {
                            out.push(RV32Inst::Lbs {
                                rd: Register::Virtual(rd_vreg),
                                symbol: symbol_id,
                            });
                        } else {
                            out.push(RV32Inst::Lbu {
                                rd: Register::Virtual(rd_vreg),
                                rs1,
                                imm: 0,
                            });
                        }
                    }
                    2 => {
                        if let Some(symbol_id) = symbol_id {
                            out.push(RV32Inst::Lhs {
                                rd: Register::Virtual(rd_vreg),
                                symbol: symbol_id,
                            });
                        } else {
                            out.push(RV32Inst::Lhu {
                                rd: Register::Virtual(rd_vreg),
                                rs1,
                                imm: 0,
                            });
                        }
                    }
                    4 => {
                        if let Some(symbol_id) = symbol_id {
                            out.push(RV32Inst::Lws {
                                rd: Register::Virtual(rd_vreg),
                                symbol: symbol_id,
                            });
                        } else {
                            out.push(RV32Inst::Lw {
                                rd: Register::Virtual(rd_vreg),
                                rs1,
                                imm: 0,
                            });
                        }
                    }
                    _ => panic!("unsupported load size"),
                }
                state.record_vreg(instruction, rd_vreg);
            }
            Ret { is_void } => {
                if !*is_void {
                    let rs = lower_operand(&operands[0], &mut out, state, machine_function)?;
                    out.push(RV32Inst::Mv {
                        rd: Register::Physical(RV32Reg::A0),
                        rs,
                    });
                }
                out.push(RV32Inst::Ret);
            }
            Store => {
                let rs2 = lower_operand(&operands[0], &mut out, state, machine_function)?;
                let addr = &operands[1];
                let type_layout = module.get_type_layout(operands[0].get_type()).unwrap();
                if addr.kind.is_global_object() {
                    let name = addr.get_name().unwrap();
                    let symbol = machine_module.symbol_map[&name];
                    match type_layout.layout.size {
                        1 => {
                            out.push(RV32Inst::Sbs { rs: rs2, symbol });
                        }
                        2 => {
                            out.push(RV32Inst::Shs { rs: rs2, symbol });
                        }
                        4 => {
                            out.push(RV32Inst::Sws { rs: rs2, symbol });
                        }
                        _ => panic!("unsupported store size"),
                    }
                } else {
                    let rs1 = lower_operand(addr, &mut out, state, machine_function)?;
                    match type_layout.layout.size {
                        1 => {
                            out.push(RV32Inst::Sb { rs1, rs2, imm: 0 });
                        }
                        2 => {
                            out.push(RV32Inst::Sh { rs1, rs2, imm: 0 });
                        }
                        4 => {
                            out.push(RV32Inst::Sw { rs1, rs2, imm: 0 });
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

                fn lower_eq(
                    rd: Register<RV32Reg>,
                    rs1: Register<RV32Reg>,
                    rs2: Register<RV32Reg>,
                    out: &mut Vec<RV32Inst>,
                    machine_function: &mut MachineFunction<RV32Arch>,
                ) {
                    let temp_vreg = machine_function.new_vreg();
                    let temp_reg = Register::Virtual(temp_vreg);
                    out.push(RV32Inst::Xor {
                        rd: temp_reg,
                        rs1,
                        rs2,
                    });
                    out.push(RV32Inst::Sltiu {
                        rd,
                        rs1: temp_reg,
                        imm: 1,
                    });
                }

                fn lower_ne(
                    rd: Register<RV32Reg>,
                    rs1: Register<RV32Reg>,
                    rs2: Register<RV32Reg>,
                    out: &mut Vec<RV32Inst>,
                    machine_function: &mut MachineFunction<RV32Arch>,
                ) {
                    let temp_vreg = machine_function.new_vreg();
                    let temp_reg = Register::Virtual(temp_vreg);
                    out.push(RV32Inst::Xor {
                        rd: temp_reg,
                        rs1,
                        rs2,
                    });
                    out.push(RV32Inst::Sltu {
                        rd,
                        rs1: Register::Physical(RV32Reg::Zero),
                        rs2: temp_reg,
                    });
                }

                fn lower_unsigned_greater_than(
                    rd: Register<RV32Reg>,
                    rs1: Register<RV32Reg>,
                    rs2: Register<RV32Reg>,
                    out: &mut Vec<RV32Inst>,
                    _machine_function: &mut MachineFunction<RV32Arch>,
                ) {
                    out.push(RV32Inst::Sltu {
                        rd,
                        rs1: rs2,
                        rs2: rs1,
                    });
                }

                fn lower_unsigned_greater_equal(
                    rd: Register<RV32Reg>,
                    rs1: Register<RV32Reg>,
                    rs2: Register<RV32Reg>,
                    out: &mut Vec<RV32Inst>,
                    machine_function: &mut MachineFunction<RV32Arch>,
                ) {
                    let tmp = Register::Virtual(machine_function.new_vreg());
                    out.push(RV32Inst::Sltu { rd: tmp, rs1, rs2 });
                    out.push(RV32Inst::Xori {
                        rd,
                        rs1: tmp,
                        imm: 1,
                    });
                }

                fn lower_unsigned_less_than(
                    rd: Register<RV32Reg>,
                    rs1: Register<RV32Reg>,
                    rs2: Register<RV32Reg>,
                    out: &mut Vec<RV32Inst>,
                    _machine_function: &mut MachineFunction<RV32Arch>,
                ) {
                    out.push(RV32Inst::Sltu { rd, rs1, rs2 });
                }

                fn lower_unsigned_less_equal(
                    rd: Register<RV32Reg>,
                    rs1: Register<RV32Reg>,
                    rs2: Register<RV32Reg>,
                    out: &mut Vec<RV32Inst>,
                    machine_function: &mut MachineFunction<RV32Arch>,
                ) {
                    let t1_vreg = machine_function.new_vreg();
                    let tmp = Register::Virtual(t1_vreg);
                    lower_unsigned_less_than(tmp, rs2, rs1, out, machine_function);
                    out.push(RV32Inst::Xori {
                        rd,
                        rs1: tmp,
                        imm: 1,
                    });
                }

                fn lower_signed_greater_than(
                    rd: Register<RV32Reg>,
                    rs1: Register<RV32Reg>,
                    rs2: Register<RV32Reg>,
                    out: &mut Vec<RV32Inst>,
                    _machine_function: &mut MachineFunction<RV32Arch>,
                ) {
                    out.push(RV32Inst::Slt {
                        rd,
                        rs1: rs2,
                        rs2: rs1,
                    });
                }

                fn lower_signed_greater_equal(
                    rd: Register<RV32Reg>,
                    rs1: Register<RV32Reg>,
                    rs2: Register<RV32Reg>,
                    out: &mut Vec<RV32Inst>,
                    machine_function: &mut MachineFunction<RV32Arch>,
                ) {
                    let tmp = Register::Virtual(machine_function.new_vreg());
                    out.push(RV32Inst::Slt { rd: tmp, rs1, rs2 });
                    out.push(RV32Inst::Xori {
                        rd,
                        rs1: tmp,
                        imm: 1,
                    });
                }

                fn lower_signed_less_than(
                    rd: Register<RV32Reg>,
                    rs1: Register<RV32Reg>,
                    rs2: Register<RV32Reg>,
                    out: &mut Vec<RV32Inst>,
                    _machine_function: &mut MachineFunction<RV32Arch>,
                ) {
                    out.push(RV32Inst::Slt { rd, rs1, rs2 });
                }

                fn lower_signed_less_equal(
                    rd: Register<RV32Reg>,
                    rs1: Register<RV32Reg>,
                    rs2: Register<RV32Reg>,
                    out: &mut Vec<RV32Inst>,
                    machine_function: &mut MachineFunction<RV32Arch>,
                ) {
                    let t1_vreg = machine_function.new_vreg();
                    let tmp = Register::Virtual(t1_vreg);
                    lower_signed_less_than(tmp, rs2, rs1, out, machine_function);
                    out.push(RV32Inst::Xori {
                        rd,
                        rs1: tmp,
                        imm: 1,
                    });
                }

                match icmp_code {
                    ICmpCode::Eq => {
                        lower_eq(rd, rs1, rs2, &mut out, machine_function);
                    }
                    ICmpCode::Ne => {
                        lower_ne(rd, rs1, rs2, &mut out, machine_function);
                    }
                    ICmpCode::Ugt => {
                        lower_unsigned_greater_than(rd, rs1, rs2, &mut out, machine_function)
                    }
                    ICmpCode::Uge => {
                        lower_unsigned_greater_equal(rd, rs1, rs2, &mut out, machine_function)
                    }
                    ICmpCode::Ult => {
                        lower_unsigned_less_than(rd, rs1, rs2, &mut out, machine_function)
                    }
                    ICmpCode::Ule => {
                        lower_unsigned_less_equal(rd, rs1, rs2, &mut out, machine_function)
                    }
                    ICmpCode::Sgt => {
                        lower_signed_greater_than(rd, rs1, rs2, &mut out, machine_function)
                    }
                    ICmpCode::Sge => {
                        lower_signed_greater_equal(rd, rs1, rs2, &mut out, machine_function)
                    }
                    ICmpCode::Slt => {
                        lower_signed_less_than(rd, rs1, rs2, &mut out, machine_function)
                    }
                    ICmpCode::Sle => {
                        lower_signed_less_equal(rd, rs1, rs2, &mut out, machine_function)
                    }
                }

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
                out.push(RV32Inst::Sub {
                    rd: Register::Virtual(mask1),
                    rs1: Register::Physical(RV32Reg::Zero),
                    rs2: cond,
                });
                out.push(RV32Inst::Xori {
                    rd: Register::Virtual(mask2),
                    rs1: Register::Virtual(mask1),
                    imm: -1,
                });

                let true_part = machine_function.new_vreg();
                let false_part = machine_function.new_vreg();
                out.push(RV32Inst::And {
                    rd: Register::Virtual(true_part),
                    rs1: true_val,
                    rs2: Register::Virtual(mask1),
                });
                out.push(RV32Inst::And {
                    rd: Register::Virtual(false_part),
                    rs1: false_val,
                    rs2: Register::Virtual(mask2),
                });
                let rd_vreg = machine_function.new_vreg();
                out.push(RV32Inst::Or {
                    rd: Register::Virtual(rd_vreg),
                    rs1: Register::Virtual(true_part),
                    rs2: Register::Virtual(false_part),
                });
                state.record_vreg(instruction, rd_vreg);
            }
            PtrToInt => {
                let src = lower_operand(&operands[0], &mut out, state, machine_function)?;
                let rd_vreg = machine_function.new_vreg();
                out.push(RV32Inst::Mv {
                    rd: Register::Virtual(rd_vreg),
                    rs: src,
                });
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
                    out.push(RV32Inst::Mv {
                        rd: Register::Virtual(rd_vreg),
                        rs: src,
                    });
                } else {
                    let shift = 32 - src_bits;
                    out.push(RV32Inst::Slli {
                        rd: Register::Virtual(rd_vreg),
                        rs1: src,
                        imm: shift as i32,
                    });
                    out.push(RV32Inst::Srai {
                        rd: Register::Virtual(rd_vreg),
                        rs1: Register::Virtual(rd_vreg),
                        imm: shift as i32,
                    });
                }
                state.record_vreg(instruction, rd_vreg);
            }
            Unreachable => {}
        };
        Ok(out)
    }

    fn ensure_symbol_absent(
        &self,
        machine_module: &MachineModule<RV32Arch>,
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
        machine_function: &mut MachineFunction<RV32Arch>,
        state: &mut FunctionLoweringState,
    ) {
        let params = &function.as_function().params;

        let mut insts = params
            .iter()
            .enumerate()
            .map(|(index, param)| {
                let vreg = machine_function.new_vreg();
                state.record_vreg(param, vreg);
                if index < 8 {
                    RV32Inst::Mv {
                        rd: Register::Virtual(vreg),
                        rs: Register::Physical(RV32Reg::reg_a(index)),
                    }
                } else {
                    RV32Inst::LoadIncomingArg {
                        rd: Register::Virtual(vreg),
                        offset: ((index - 8) * 4) as i32,
                    }
                }
            })
            .collect();

        machine_function
            .get_block_mut(machine_function.entry)
            .unwrap()
            .instructions
            .append(&mut insts);
    }
}

fn empty_machine_function(name: String) -> MachineFunction<RV32Arch> {
    MachineFunction {
        name,
        blocks: Vec::new(),
        next_vreg_id: 0,
        entry: BlockId(0),
        frame_info: FrameInfo {
            stack_slots: Vec::new(),
            max_align: 1,
            max_outgoing_arg_size: 0,
        },
        frame_layout: FrameLayout {
            frame_size: 0,
            slot_offsets: HashMap::new(),
            outgoing_arg_offset: 0,
            incoming_arg_offset: 0,
        },
    }
}
