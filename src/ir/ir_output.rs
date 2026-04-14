use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

use crate::ir::{
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
