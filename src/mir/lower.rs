use std::{
    collections::HashMap,
    error::Error,
    fmt::{self, Display, Formatter},
    ops::Deref,
    rc::Rc,
};

use crate::{
    ir::{
        LLVMModule,
        globalxxx::{FunctionPtr, GlobalVariablePtr},
        ir_value::{
            BasicBlockPtr, Constant, ConstantArray, ConstantInt, ConstantPtr, ConstantString,
            ConstantStruct, InstructionPtr, Value,
        },
        layout::TypeLayout,
    },
    mir::{
        BlockId, FrameInfo, FrameLayout, Linkage, MachineBlock, MachineFunction, MachineModule,
        MachineSegment, MachineSymbolKind, StackSlotId, SymbolId, VRegId,
        rv32im::{RV32Arch, RV32Inst},
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
}

impl FunctionLoweringState {
    fn new(function_name: String) -> Self {
        Self {
            function_name,
            block_map: HashMap::new(),
            block_order: Vec::new(),
            value_vregs: HashMap::new(),
            stack_slots: HashMap::new(),
        }
    }

    fn record_block(&mut self, block: &BasicBlockPtr, id: BlockId) {
        self.block_map.insert(basic_block_key(block), id);
        self.block_order.push(block.clone());
    }

    fn block_id(&self, block: &BasicBlockPtr) -> Option<BlockId> {
        self.block_map.get(&basic_block_key(block)).copied()
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

            let name = function.get_name().ok_or(LowerError::MissingFunctionName)?;
            let symbol_id = *state
                .function_symbols
                .get(&name)
                .ok_or_else(|| LowerError::UnknownFunctionSymbol(name.clone()))?;
            let lowered = self.lower_function(&function)?;
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
    ) -> Result<MachineFunction<RV32Arch>, LowerError> {
        let function_name = function.get_name().ok_or(LowerError::MissingFunctionName)?;
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
        machine_function.entry =
            state
                .block_id(&entry)
                .ok_or_else(|| LowerError::UnknownBlock {
                    function: function_name.clone(),
                    block_name: entry.get_name(),
                })?;

        for block in &blocks {
            self.lower_block(block, &mut machine_function, &mut state)?;
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
        state: &mut FunctionLoweringState,
    ) -> Result<(), LowerError> {
        let block_id = state
            .block_id(block)
            .ok_or_else(|| LowerError::UnknownBlock {
                function: state.function_name.clone(),
                block_name: block.get_name(),
            })?;
        let instructions = block.as_basic_block().instructions.borrow().clone();
        let out = &mut machine_function.blocks[block_id.0].instructions;

        for instruction in &instructions {
            self.lower_instruction(instruction, out, state)?;
        }

        Ok(())
    }

    fn lower_instruction(
        &mut self,
        instruction: &InstructionPtr,
        out: &mut Vec<RV32Inst>,
        state: &mut FunctionLoweringState,
    ) -> Result<(), LowerError> {
        let _ = out;
        let _ = &state.value_vregs;
        let _ = &state.stack_slots;

        Err(LowerError::UnimplementedInstruction {
            function: state.function_name.clone(),
            opcode: instruction
                .as_instruction()
                .get_instruction_name()
                .to_string(),
        })
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
        },
    }
}

fn basic_block_key(block: &BasicBlockPtr) -> *const Value {
    value_ptr_key(block.deref())
}

fn value_ptr_key(value: &Rc<Value>) -> *const Value {
    Rc::as_ptr(value)
}
