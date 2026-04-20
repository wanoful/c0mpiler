use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

use crate::ir::{
    core::{BlockRef, FunctionId, InstRef, ModuleCore, ValueId},
    core_inst::{BinaryOpcode, ICmpCode, InstKind},
    core_value::ConstKind,
    LLVMModule,
    attribute::{Attribute, AttributeList},
    globalxxx::{FunctionPtr, GlobalVariablePtr},
    ir_type::{Type, TypePtr},
    ir_value::{BasicBlockPtr, Constant, ConstantPtr, InstructionPtr, Value, ValuePtr},
    layout::TargetDataLayout,
};

const IR_INDENT_NUM: usize = 4;

#[derive(Default)]
struct PrintHelper {
    result: String,

    used_named_struct: HashSet<String>,

    // 局部变量和 label 共用命名空间，全局 namespace 不能重名
    local_name_space: HashSet<String>,
    local_rename_map: HashMap<*const Value, String>,
    core_local_rename_map: HashMap<ValueId, String>,
    core_block_rename_map: HashMap<BlockRef, String>,

    indent: usize,
    value_with_type: bool,
    no_struct_type_alias: bool,

    top_level_strings: String,
}

impl PrintHelper {
    fn add_struct_defination(&mut self, map: &HashMap<String, TypePtr>) {
        let mut helper = PrintHelper {
            used_named_struct: self.used_named_struct.clone(),
            ..Default::default()
        };

        let mut diff = self.used_named_struct.clone();

        while !diff.is_empty() {
            for x in diff {
                helper.no_struct_type_alias = true;
                helper.append_white(&format!("%{} = type", x));
                map.get(&x).unwrap().ir_print(&mut helper);
                helper.appendln("");
            }
            diff = helper
                .used_named_struct
                .difference(&self.used_named_struct)
                .cloned()
                .collect();
            self.used_named_struct = helper.used_named_struct.clone();
        }

        self.result = helper.result + &self.result;
    }

    fn append(&mut self, s: &str) {
        self.result += s;
    }

    fn append_white(&mut self, s: &str) {
        self.result += s;
        self.result += " ";
    }

    fn appendln(&mut self, s: &str) {
        self.result += s;
        self.result += "\n";
        self.result += " ".repeat(self.indent * IR_INDENT_NUM).as_str();
    }

    fn append_top_levelln(&mut self, s: &str) {
        self.top_level_strings += s;
        self.top_level_strings += "\n";
    }

    fn clear_local_name_space(&mut self) {
        self.local_name_space.clear();
        self.local_rename_map.clear();
        self.core_local_rename_map.clear();
        self.core_block_rename_map.clear();
    }

    fn increase_indent(&mut self) {
        self.indent += 1;
    }

    fn decrease_indent(&mut self) {
        if self.indent > 0 {
            self.indent -= 1;

            if self.result.ends_with(&" ".repeat(IR_INDENT_NUM)) {
                self.result.truncate(self.result.len() - IR_INDENT_NUM);
            }
        }
    }

    fn seek_available_name(raw_name: Option<String>, space: &mut HashSet<String>) -> String {
        let raw_name = raw_name.unwrap_or("".to_string());
        let mut name = if raw_name.is_empty() {
            "0".to_string()
        } else {
            raw_name.clone()
        };
        let mut count: usize = 0;

        while space.contains(&name) {
            count += 1;
            name = format!("{raw_name}{count}");
        }

        space.insert(name.clone());

        name
    }

    fn intern_local_name(&mut self, value: &ValuePtr) -> String {
        let raw_ptr = Rc::as_ptr(value);
        if let Some(res) = self.local_rename_map.get(&raw_ptr) {
            res.clone()
        } else {
            let name = Self::seek_available_name(value.get_name(), &mut self.local_name_space);
            self.local_rename_map.insert(raw_ptr, name.clone());
            name
        }
    }

    fn intern_core_value_name(&mut self, value: ValueId, raw_name: Option<String>) -> String {
        if let Some(res) = self.core_local_rename_map.get(&value) {
            res.clone()
        } else {
            let name = Self::seek_available_name(raw_name, &mut self.local_name_space);
            self.core_local_rename_map.insert(value, name.clone());
            name
        }
    }

    fn intern_core_block_name(&mut self, block: BlockRef, raw_name: Option<String>) -> String {
        if let Some(res) = self.core_block_rename_map.get(&block) {
            res.clone()
        } else {
            let name = Self::seek_available_name(raw_name, &mut self.local_name_space);
            self.core_block_rename_map.insert(block, name.clone());
            name
        }
    }

    fn get_result(self) -> String {
        self.top_level_strings + "\n" + &self.result
    }
}

trait IRPrint {
    fn ir_print(&self, helper: &mut PrintHelper);
}

impl LLVMModule {
    pub fn print(&self) -> String {
        let mut helper = PrintHelper::default();
        self.ir_print(&mut helper);
        let ctx = self.ctx_impl.borrow();
        helper.add_struct_defination(&ctx.named_strcut_ty);
        helper.get_result()
    }
}

impl ModuleCore {
    pub fn print(&self) -> String {
        let mut helper = PrintHelper::default();
        if let Some(target) = self.target_data_layout {
            target.ir_print(&mut helper);
        }
        self.ir_print(&mut helper);
        helper.add_struct_defination(&self.named_structs);
        helper.get_result()
    }
}

impl IRPrint for TargetDataLayout {
    fn ir_print(&self, helper: &mut PrintHelper) {
        helper.append_top_levelln(&format!(
            "target datalayout = \"{}\"",
            self.llvm_data_layout()
        ));
    }
}

impl IRPrint for LLVMModule {
    fn ir_print(&self, helper: &mut PrintHelper) {
        let ctx = self.ctx_impl.borrow();
        let layout = &ctx.type_layout_engine.target();
        layout.ir_print(helper);
        drop(ctx);

        self.global_variables.iter().for_each(|(_, v)| {
            v.ir_print(helper);
            helper.appendln("");
        });

        helper.appendln("");

        let mut functions = self.functions.values().collect::<Vec<_>>();
        functions.sort_by_key(|(_, x)| x);

        functions.iter().for_each(|(func, _)| {
            func.ir_print(helper);
            helper.appendln("");
        });
    }
}

impl IRPrint for ModuleCore {
    fn ir_print(&self, helper: &mut PrintHelper) {
        for global in self.globals_in_order() {
            if self.global(global).kind.as_function().is_none() {
                self.ir_print_global(global, helper);
                helper.appendln("");
            }
        }

        if !self.globals_in_order().is_empty() {
            helper.appendln("");
        }

        for func in self.functions_in_order() {
            self.ir_print_function(func, helper);
            helper.appendln("");
        }
    }
}

impl ModuleCore {
    fn pre_intern_function_names(&self, func: FunctionId, helper: &mut PrintHelper) {
        for arg in self.args_in_order(func) {
            let raw = self.arg(arg).name.clone();
            helper.intern_core_value_name(ValueId::Arg(arg), raw);
        }

        for block in self.blocks_in_order(func) {
            let raw = self.block(block).name.clone();
            helper.intern_core_block_name(block, raw);
        }

        for block in self.blocks_in_order(func) {
            for inst in self.phis_in_order(block) {
                if !self.inst(inst).ty.is_void() {
                    let raw = self.inst(inst).name.clone();
                    helper.intern_core_value_name(ValueId::Inst(inst), raw);
                }
            }
            for inst in self.insts_in_order(block) {
                if !self.inst(inst).ty.is_void() {
                    let raw = self.inst(inst).name.clone();
                    helper.intern_core_value_name(ValueId::Inst(inst), raw);
                }
            }
        }
    }

    fn ir_print_function(&self, func: FunctionId, helper: &mut PrintHelper) {
        let func_data = self.func(func);
        let func_ty = &func_data.ty;
        let ret_ty = func_ty.0.as_function().unwrap().0.clone();

        helper.append_white(if func_data.is_declare { "declare" } else { "define" });
        ret_ty.as_ref().ir_print(helper);
        helper.append_white("");
        helper.append(&format!("@{}", func_data.name));
        helper.append("(");

        let args = self.args_in_order(func);
        for (i, arg) in args.iter().enumerate() {
            if i > 0 {
                helper.append(", ");
            }
            self.arg(*arg).ty.as_ref().ir_print(helper);
            helper.append(" ");
            if i == 0 && let Some(sret_ty) = &func_data.sret {
                helper.append("sret(");
                sret_ty.as_ref().ir_print(helper);
                helper.append(") ");
            }
            let name = helper.intern_core_value_name(ValueId::Arg(*arg), self.arg(*arg).name.clone());
            helper.append(&format!("%{}", name));
        }

        helper.append(")");
        if func_data.is_declare {
            return;
        }
        helper.append(" ");
        helper.increase_indent();
        helper.appendln("{");

        self.pre_intern_function_names(func, helper);

        for block in self.blocks_in_order(func) {
            self.ir_print_block(block, helper);
        }

        helper.decrease_indent();
        helper.appendln("}");
        helper.clear_local_name_space();
    }

    fn ir_print_global(&self, global: crate::ir::core::GlobalId, helper: &mut PrintHelper) {
        let global_data = self.global(global);
        if let crate::ir::core_value::GlobalKind::GlobalVariable {
            is_constant,
            initializer,
        } = &global_data.kind
        {
            helper.append(&format!("@{} = ", global_data.name));
            helper.append(if *is_constant { "constant " } else { "global " });
            if let Some(initializer) = initializer {
                self.const_data(*initializer).ty.as_ref().ir_print(helper);
                helper.append(" ");
                self.ir_print_const(*initializer, helper);
            } else {
                global_data.ty.as_ref().ir_print(helper);
                helper.append(" ");
                helper.append("zeroinitializer");
            }
        }
    }

    fn ir_print_block(&self, block: BlockRef, helper: &mut PrintHelper) {
        let name = helper.intern_core_block_name(block, self.block(block).name.clone());
        helper.appendln(&format!("{}:", name));

        for inst in self.phis_in_order(block) {
            self.ir_print_inst(inst, helper);
            helper.appendln("");
        }
        for inst in self.insts_in_order(block) {
            self.ir_print_inst(inst, helper);
            helper.appendln("");
        }
        if let Some(inst) = self.terminator(block) {
            self.ir_print_inst(inst, helper);
            helper.appendln("");
        }
    }

    fn ir_print_value(&self, value: ValueId, helper: &mut PrintHelper) {
        match value {
            ValueId::Inst(inst) => {
                let name = helper.intern_core_value_name(value, self.inst(inst).name.clone());
                helper.append(&format!("%{}", name));
            }
            ValueId::Arg(arg) => {
                let name = helper.intern_core_value_name(value, self.arg(arg).name.clone());
                helper.append(&format!("%{}", name));
            }
            ValueId::Global(global) => {
                helper.append(&format!("@{}", self.global(global).name));
            }
            ValueId::Const(const_id) => {
                self.ir_print_const(const_id, helper);
            }
        }
    }

    fn ir_print_typed_value(&self, value: ValueId, helper: &mut PrintHelper) {
        self.value_ty(value).as_ref().ir_print(helper);
        helper.append(" ");
        self.ir_print_value(value, helper);
    }

    fn ir_print_block_ref(&self, block: BlockRef, helper: &mut PrintHelper) {
        let name = helper.intern_core_block_name(block, self.block(block).name.clone());
        helper.append(&format!("%{}", name));
    }

    fn ir_print_const(&self, const_id: crate::ir::core::ConstId, helper: &mut PrintHelper) {
        match &self.const_data(const_id).kind {
            ConstKind::Int(value) => helper.append(&value.to_string()),
            ConstKind::Array(values) => {
                helper.append("[");
                for (i, value) in values.iter().enumerate() {
                    if i > 0 {
                        helper.append(", ");
                    }
                    self.const_data(*value).ty.as_ref().ir_print(helper);
                    helper.append(" ");
                    self.ir_print_const(*value, helper);
                }
                helper.append("]");
            }
            ConstKind::Struct(values) => {
                helper.append("{");
                for (i, value) in values.iter().enumerate() {
                    if i > 0 {
                        helper.append(", ");
                    }
                    self.const_data(*value).ty.as_ref().ir_print(helper);
                    helper.append(" ");
                    self.ir_print_const(*value, helper);
                }
                helper.append("}");
            }
            ConstKind::String(value) => {
                helper.append(&format!(r##"c"{}""##, bytes_escape(value)));
            }
            ConstKind::Null => helper.append("null"),
        }
    }

    fn ir_print_inst(&self, inst: InstRef, helper: &mut PrintHelper) {
        let inst_data = self.inst(inst);
        if !inst_data.ty.is_void() {
            let name = helper.intern_core_value_name(ValueId::Inst(inst), inst_data.name.clone());
            helper.append_white(&format!("%{} =", name));
        }

        match &inst_data.kind {
            InstKind::Alloca { ty } => {
                helper.append_white("alloca");
                ty.as_ref().ir_print(helper);
            }
            InstKind::Load { ptr } => {
                helper.append_white("load");
                inst_data.ty.as_ref().ir_print(helper);
                helper.append(", ");
                self.ir_print_typed_value(*ptr, helper);
            }
            InstKind::Store { value, ptr } => {
                helper.append_white("store");
                self.ir_print_typed_value(*value, helper);
                helper.append(", ");
                self.ir_print_typed_value(*ptr, helper);
            }
            InstKind::Binary { op, lhs, rhs } => {
                helper.append_white(match op {
                    BinaryOpcode::Add => "add",
                    BinaryOpcode::Sub => "sub",
                    BinaryOpcode::Mul => "mul",
                    BinaryOpcode::UDiv => "udiv",
                    BinaryOpcode::SDiv => "sdiv",
                    BinaryOpcode::URem => "urem",
                    BinaryOpcode::SRem => "srem",
                    BinaryOpcode::Shl => "shl",
                    BinaryOpcode::LShr => "lshr",
                    BinaryOpcode::AShr => "ashr",
                    BinaryOpcode::And => "and",
                    BinaryOpcode::Or => "or",
                    BinaryOpcode::Xor => "xor",
                });
                self.value_ty(*lhs).as_ref().ir_print(helper);
                helper.append(" ");
                self.ir_print_value(*lhs, helper);
                helper.append(", ");
                self.ir_print_value(*rhs, helper);
            }
            InstKind::ICmp { op, lhs, rhs } => {
                helper.append_white("icmp");
                helper.append_white(match op {
                    ICmpCode::Eq => "eq",
                    ICmpCode::Ne => "ne",
                    ICmpCode::Ugt => "ugt",
                    ICmpCode::Uge => "uge",
                    ICmpCode::Ult => "ult",
                    ICmpCode::Ule => "ule",
                    ICmpCode::Sgt => "sgt",
                    ICmpCode::Sge => "sge",
                    ICmpCode::Slt => "slt",
                    ICmpCode::Sle => "sle",
                });
                self.value_ty(*lhs).as_ref().ir_print(helper);
                helper.append(" ");
                self.ir_print_value(*lhs, helper);
                helper.append(", ");
                self.ir_print_value(*rhs, helper);
            }
            InstKind::Call { func, args } => {
                helper.append_white("call");
                self.func(*func)
                    .ty
                    .0
                    .as_function()
                    .unwrap()
                    .0
                    .as_ref()
                    .ir_print(helper);
                helper.append(" ");
                helper.append(&format!("@{}(", self.func(*func).name));
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        helper.append(", ");
                    }
                    self.ir_print_typed_value(*arg, helper);
                }
                helper.append(")");
            }
            InstKind::GetElementPtr {
                base_ty,
                base,
                indices,
            } => {
                helper.append_white("getelementptr");
                base_ty.as_ref().ir_print(helper);
                helper.append(", ");
                self.ir_print_typed_value(*base, helper);
                for index in indices {
                    helper.append(", ");
                    self.ir_print_typed_value(*index, helper);
                }
            }
            InstKind::Phi { incomings } => {
                helper.append_white("phi");
                inst_data.ty.as_ref().ir_print(helper);
                helper.append(" ");
                for (i, incoming) in incomings.iter().enumerate() {
                    if i > 0 {
                        helper.append(", ");
                    }
                    helper.append("[ ");
                    self.ir_print_value(incoming.value, helper);
                    helper.append(", ");
                    self.ir_print_block_ref(
                        BlockRef {
                            func: inst.func,
                            block: incoming.block,
                        },
                        helper,
                    );
                    helper.append(" ]");
                }
            }
            InstKind::Select {
                cond,
                then_val,
                else_val,
            } => {
                helper.append_white("select");
                self.ir_print_typed_value(*cond, helper);
                helper.append(", ");
                self.ir_print_typed_value(*then_val, helper);
                helper.append(", ");
                self.ir_print_typed_value(*else_val, helper);
            }
            InstKind::PtrToInt { ptr } => {
                helper.append_white("ptrtoint");
                self.ir_print_typed_value(*ptr, helper);
                helper.append_white("to");
                inst_data.ty.as_ref().ir_print(helper);
            }
            InstKind::Trunc { value } => {
                helper.append_white("trunc");
                self.ir_print_typed_value(*value, helper);
                helper.append_white("to");
                inst_data.ty.as_ref().ir_print(helper);
            }
            InstKind::Zext { value } => {
                helper.append_white("zext");
                self.ir_print_typed_value(*value, helper);
                helper.append_white("to");
                inst_data.ty.as_ref().ir_print(helper);
            }
            InstKind::Sext { value } => {
                helper.append_white("sext");
                self.ir_print_typed_value(*value, helper);
                helper.append_white("to");
                inst_data.ty.as_ref().ir_print(helper);
            }
            InstKind::Branch { then_block, cond } => {
                helper.append_white("br");
                if let Some(cond) = cond {
                    self.ir_print_typed_value(cond.cond, helper);
                    helper.append(", label ");
                    self.ir_print_block_ref(
                        BlockRef {
                            func: inst.func,
                            block: *then_block,
                        },
                        helper,
                    );
                    helper.append(", label ");
                    self.ir_print_block_ref(
                        BlockRef {
                            func: inst.func,
                            block: cond.else_block,
                        },
                        helper,
                    );
                } else {
                    helper.append("label ");
                    self.ir_print_block_ref(
                        BlockRef {
                            func: inst.func,
                            block: *then_block,
                        },
                        helper,
                    );
                }
            }
            InstKind::Ret { value } => {
                helper.append_white("ret");
                if let Some(value) = value {
                    self.ir_print_typed_value(*value, helper);
                } else {
                    helper.append("void");
                }
            }
            InstKind::Unreachable => helper.append("unreachable"),
        }
    }
}

// 对 ValuePtr 打印则只打印 name，对具体的 Ptr 打印才打印内部结构
impl IRPrint for ValuePtr {
    fn ir_print(&self, helper: &mut PrintHelper) {
        if helper.value_with_type {
            self.get_type().ir_print(helper);
            helper.append_white("");
        }

        match &self.kind {
            crate::ir::ir_value::ValueKind::BasicBlock(_)
            | crate::ir::ir_value::ValueKind::Argument(_)
            | crate::ir::ir_value::ValueKind::Instruction(_) => {
                let name = helper.intern_local_name(self);
                helper.append(&format!("%{}", name));
            }
            crate::ir::ir_value::ValueKind::Constant(constant) => {
                constant.ir_print(helper);
            }
            crate::ir::ir_value::ValueKind::GlobalObject(_) => {
                helper.append(&format!("@{}", self.get_name().unwrap()));
            }
        }
    }
}

impl IRPrint for ConstantPtr {
    fn ir_print(&self, helper: &mut PrintHelper) {
        if helper.value_with_type {
            self.get_type().ir_print(helper);
            helper.append_white("");
        }

        self.as_constant().ir_print(helper);
    }
}

impl IRPrint for FunctionPtr {
    fn ir_print(&self, helper: &mut PrintHelper) {
        let func = self.as_function();
        let args = func.args();
        let attr = func.attr.borrow();
        let blocks = func.blocks.borrow();
        let is_declare = blocks.is_empty();

        helper.append_white(if is_declare { "declare" } else { "define" });

        let func_type = self
            .as_global_object()
            .get_inner_ty()
            .as_function()
            .unwrap();
        let ret_type = &func_type.0;
        (&attr.ret_attr, ret_type.as_ref()).ir_print(helper);
        helper.append_white("");

        self.0.ir_print(helper);
        helper.append_white("");

        helper.append("(");
        let arg_outs: Vec<_> = args
            .iter()
            .zip(attr.params_attr.iter())
            .map(|(a, b)| (a.get_type().as_ref(), " ", b, &a.0))
            .collect();
        arg_outs.ir_print(helper);
        helper.append_white(")");

        attr.fn_attr.ir_print(helper);

        if !is_declare {
            helper.increase_indent();
            helper.appendln("{");

            blocks.iter().for_each(|block| block.pre_intern(helper));
            blocks.iter().for_each(|block| block.ir_print(helper));

            helper.decrease_indent();
            helper.appendln("}");
        }

        helper.clear_local_name_space();
    }
}

impl IRPrint for GlobalVariablePtr {
    fn ir_print(&self, helper: &mut PrintHelper) {
        helper.append_white(&format!("@{} =", self.get_name().unwrap()));
        helper.append_white(if self.as_global_variable().is_constant {
            "constant"
        } else {
            "global"
        });

        helper.value_with_type = true;
        self.as_global_variable().initializer.ir_print(helper);
        helper.value_with_type = false;
    }
}

impl IRPrint for BasicBlockPtr {
    fn ir_print(&self, helper: &mut PrintHelper) {
        let name = helper.intern_local_name(self);
        helper.appendln(&format!("{}:", name));

        let ins_ref = self.as_basic_block().instructions.borrow();
        for ins in ins_ref.iter() {
            ins.ir_print(helper);
            helper.appendln("");
        }
    }
}

impl BasicBlockPtr {
    fn pre_intern(&self, helper: &mut PrintHelper) {
        let instructions = self.as_basic_block().instructions.borrow();
        instructions.iter().for_each(|x| {
            if !x.get_type().is_void() {
                helper.intern_local_name(x);
            }
        });
    }
}

impl IRPrint for InstructionPtr {
    fn ir_print(&self, helper: &mut PrintHelper) {
        use super::ir_value::InstructionKind::*;

        let ins = self.as_instruction();
        let operands = &ins.operands;

        if !self.get_type().is_void() {
            let name = helper.intern_local_name(self);
            helper.append_white(&format!("%{name} ="));
        }

        helper.append_white(self.as_instruction().get_instruction_name());

        match &ins.kind {
            Binary(_) => {
                self.get_type().ir_print(helper);
                helper.append_white("");
                operands.ir_print(helper);
            }
            Call => {
                self.get_type().ir_print(helper);
                helper.append_white("");

                let operands = &operands;
                let (func_ptr, args_ptr) = operands.split_first().unwrap();
                let attr = func_ptr
                    .kind
                    .as_global_object()
                    .unwrap()
                    .kind
                    .as_function()
                    .unwrap()
                    .attr
                    .borrow();

                func_ptr.ir_print(helper);

                helper.append("(");
                let args: Vec<_> = args_ptr
                    .iter()
                    .zip(attr.params_attr.iter())
                    .map(|(a, b)| (a.get_type().as_ref(), " ", b, a))
                    .collect();
                args.ir_print(helper);
                helper.append(")");
            }
            Branch { has_cond } => {
                debug_assert!((*has_cond && operands.len() == 3) || operands.len() == 1);
                helper.value_with_type = true;
                operands.ir_print(helper);
                helper.value_with_type = false;
            }
            GetElementPtr { base_ty } => {
                base_ty.ir_print(helper);
                helper.append_white(",");
                helper.value_with_type = true;
                operands.ir_print(helper);
                helper.value_with_type = false;
            }
            Alloca { inner_ty } => {
                inner_ty.ir_print(helper);
                // Alloca 暂时没有携带任何操作数
            }
            Load => {
                self.get_type().ir_print(helper);
                helper.append_white(",");
                helper.value_with_type = true;
                operands.ir_print(helper);
                helper.value_with_type = false;
            }
            Ret { is_void } => {
                if *is_void {
                    helper.append("void");
                } else {
                    helper.value_with_type = true;
                    operands.ir_print(helper);
                    helper.value_with_type = false;
                }
            }
            Store => {
                helper.value_with_type = true;
                operands.ir_print(helper);
                helper.value_with_type = false;
            }
            Icmp(icmp_code) => {
                helper.append_white(icmp_code.get_operator_name());
                operands[0].get_type().ir_print(helper);
                helper.append_white("");
                operands.ir_print(helper);
            }
            Phi => {
                self.get_type().ir_print(helper);
                helper.append_white("");

                struct PhiPrintHelper<'a>(&'a [ValuePtr]);

                impl<'a> IRPrint for PhiPrintHelper<'a> {
                    fn ir_print(&self, helper: &mut PrintHelper) {
                        helper.append("[");
                        self.0.ir_print(helper);
                        helper.append("]");
                    }
                }

                debug_assert!(operands.len().is_multiple_of(2));

                let chunked: Vec<_> = operands.chunks(2).map(PhiPrintHelper).collect();
                chunked.ir_print(helper);
            }
            Select => operands.ir_print(helper),
            PtrToInt | Trunc | Zext | Sext => {
                helper.value_with_type = true;
                operands.ir_print(helper);
                helper.value_with_type = false;
                helper.append_white("");
                helper.append_white("to");
                self.get_type().ir_print(helper);
            }
            Unreachable => {}
        }
    }
}

impl IRPrint for Type {
    fn ir_print(&self, helper: &mut PrintHelper) {
        match self {
            Type::Int(int_type) => helper.append(&format!("i{}", int_type.0)),
            Type::Function(function_type) => {
                function_type.0.ir_print(helper);
                helper.append(" (");
                function_type.1.ir_print(helper);
                helper.append(")");
            }
            Type::Ptr(_) => helper.append("ptr"),
            Type::Struct(struct_type) => {
                if !helper.no_struct_type_alias
                    && let Some(name) = struct_type.get_name()
                {
                    helper.append(&format!("%{name}"));
                    helper.used_named_struct.insert(name);
                } else {
                    helper.no_struct_type_alias = false;
                    helper.append("{");
                    struct_type.get_body().unwrap().ir_print(helper);
                    helper.append("}");
                }
            }
            Type::Array(array_type) => {
                helper.append(&format!("[{} x ", array_type.1));
                array_type.0.ir_print(helper);
                helper.append("]");
            }
            Type::Void(_) => helper.append("void"),
            Type::Label(_) => helper.append("label"),
        }
    }
}

impl<T> IRPrint for [T]
where
    T: IRPrint,
{
    fn ir_print(&self, helper: &mut PrintHelper) {
        let mut iter = self.iter();

        if let Some(x) = iter.next() {
            x.ir_print(helper)
        }

        for x in iter {
            helper.append(", ");
            x.ir_print(helper);
        }
    }
}

impl<T> IRPrint for [Rc<T>]
where
    T: IRPrint,
{
    fn ir_print(&self, helper: &mut PrintHelper) {
        let mut iter = self.iter();

        if let Some(x) = iter.next() {
            x.ir_print(helper)
        }

        for x in iter {
            helper.append(", ");
            x.ir_print(helper);
        }
    }
}

macro_rules! ir_print_for_tuple {
    ( $head:ident, ) => {};
    ( $head:ident, $( $tail:ident, )+ ) => {
        impl<$head, $( $tail ),+> IRPrint for (&$head, $( &$tail ),+)
        where
            $head: IRPrint + ?Sized,
            $( $tail: IRPrint + ?Sized ),*
        {
            #[allow(non_snake_case)]
            fn ir_print(&self, helper: &mut PrintHelper) {
                let (head, $( $tail ),+) = self;
                head.ir_print(helper);
                ($(*$tail),+).ir_print(helper);
            }
        }

        ir_print_for_tuple!($( $tail, )+);
    };

    () => {};
}

ir_print_for_tuple!(A, B, C, D, E, F, G, H, I, J,);

impl IRPrint for Constant {
    fn ir_print(&self, helper: &mut PrintHelper) {
        match self {
            Constant::ConstantInt(constant_int) => helper.append(&format!("{}", constant_int.0)),
            Constant::ConstantArray(constant_array) => {
                helper.append("[");
                let value_with_type = helper.value_with_type;
                helper.value_with_type = true;
                constant_array.0.ir_print(helper);
                helper.value_with_type = value_with_type;
                helper.append("]");
            }
            Constant::ConstantStruct(constant_struct) => {
                helper.append("{");
                let value_with_type = helper.value_with_type;
                helper.value_with_type = true;
                constant_struct.0.ir_print(helper);
                helper.value_with_type = value_with_type;
                helper.append("}");
            }
            Constant::ConstantString(constant_string) => {
                // TODO: 匿名常量的处理
                helper.append(&format!(r##"c"{}""##, bytes_escape(&constant_string.0)));
            }
            Constant::ConstantNull(_) => helper.append("null"),
        }
    }
}

fn bytes_escape(input: &str) -> String {
    let mut ret = String::new();
    for byte in input.as_bytes() {
        if byte.is_ascii_graphic() && *byte != b'"' && *byte != b'\\' {
            ret.push(char::from(*byte));
        } else {
            ret.push_str(&format!("\\{byte:X}"));
        }
    }
    ret
}

impl IRPrint for AttributeList {
    fn ir_print(&self, helper: &mut PrintHelper) {
        for x in self.defined.iter().flatten() {
            x.ir_print(helper);
            " ".ir_print(helper);
        }
    }
}

impl IRPrint for Attribute {
    fn ir_print(&self, helper: &mut PrintHelper) {
        match self {
            Attribute::StructReturn(type_ptr) => {
                helper.append("sret");
                helper.append("(");
                type_ptr.ir_print(helper);
                helper.append(")");
            }
        }
    }
}

impl IRPrint for str {
    fn ir_print(&self, helper: &mut PrintHelper) {
        helper.append(self);
    }
}
