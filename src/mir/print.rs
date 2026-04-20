use std::{
    collections::{HashMap, HashSet},
    fmt::{Debug, Display},
    hash::Hash,
};

use crate::mir::{
    BlockId, MachineFunction, MachineModule, MachineSegment, MachineSymbol, MachineSymbolKind,
    Register, SymbolId, TargetArch,
};

pub struct Renamer<T: PartialEq + Eq + Hash> {
    pub symbol_names: HashMap<T, String>,
    pub names_in_use: HashSet<String>,
}

impl<T: PartialEq + Eq + Hash> Renamer<T> {
    pub fn new() -> Self {
        Self {
            symbol_names: HashMap::new(),
            names_in_use: HashSet::new(),
        }
    }

    pub fn intern_name(&mut self, key: T, name: String) -> String {
        if let Some(existing) = self.symbol_names.get(&key) {
            return existing.clone();
        }

        if self.names_in_use.contains(&name) {
            let mut suffix = 1;
            loop {
                let new_name = format!("{}{suffix}", name);
                if !self.names_in_use.contains(&new_name) {
                    self.symbol_names.insert(key, new_name.clone());
                    self.names_in_use.insert(new_name.clone());
                    return new_name;
                }
                suffix += 1;
            }
        } else {
            self.symbol_names.insert(key, name.clone());
            self.names_in_use.insert(name.clone());
            name
        }
    }
}

pub struct ModulePrinter<'a, T: TargetArch> {
    pub module: &'a MachineModule<T>,
}

pub struct FunctionPrinter<'a, T: TargetArch> {
    pub func: &'a MachineFunction<T>,
    pub module: &'a MachineModule<T>,
}

pub trait InstPrinter<'a, T: TargetArch>: Display {
    fn new(
        inst: &'a T::MachineInst,
        block_names: &'a HashMap<BlockId, String>,
        symbol_names: &'a HashMap<SymbolId, String>,
    ) -> Self;
}

impl<T: TargetArch> Display for ModulePrinter<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.print_segment(f, MachineSegment::Text)?;
        self.print_segment(f, MachineSegment::ReadOnlyData)?;
        self.print_segment(f, MachineSegment::Data)?;
        self.print_segment(f, MachineSegment::Bss)?;
        Ok(())
    }
}

fn section_name(segment: MachineSegment) -> &'static str {
    match segment {
        MachineSegment::Text => ".text",
        MachineSegment::ReadOnlyData => ".rodata",
        MachineSegment::Data => ".data",
        MachineSegment::Bss => ".bss",
    }
}

impl<T: TargetArch> ModulePrinter<'_, T> {
    fn print_segment(
        &self,
        f: &mut std::fmt::Formatter<'_>,
        segment: MachineSegment,
    ) -> std::fmt::Result {
        let symbols = self
            .module
            .symbols
            .iter()
            .filter(|sym| sym.segment == segment)
            .collect::<Vec<_>>();

        if symbols.is_empty() {
            return Ok(());
        }

        writeln!(f, "    {}", section_name(segment))?;
        writeln!(f)?;

        let mut last_printed = false;
        for symbol in symbols.iter() {
            if last_printed {
                writeln!(f)?;
            }
            last_printed = self.print_symbol(f, symbol)?;
        }

        Ok(())
    }

    fn print_symbol(
        &self,
        f: &mut std::fmt::Formatter<'_>,
        symbol: &MachineSymbol<T>,
    ) -> Result<bool, std::fmt::Error> {
        match &symbol.kind {
            MachineSymbolKind::Function(func) => {
                self.print_symbol_header(f, symbol)?;
                write!(
                    f,
                    "{}",
                    FunctionPrinter {
                        func,
                        module: self.module
                    }
                )?;
            }
            MachineSymbolKind::ExternalPlaceholder => {
                return Ok(false);
            }
            MachineSymbolKind::Data(bytes) => {
                self.print_symbol_header(f, symbol)?;
                self.print_data_object(f, bytes)?;
            }
            MachineSymbolKind::Bss { size } => {
                self.print_symbol_header(f, symbol)?;
                writeln!(f, "    .zero {size}")?;
            }
        }
        Ok(true)
    }

    fn print_symbol_header(
        &self,
        f: &mut std::fmt::Formatter<'_>,
        symbol: &MachineSymbol<T>,
    ) -> std::fmt::Result {
        match symbol.linkage {
            crate::mir::Linkage::Internal => {}
            crate::mir::Linkage::External => writeln!(f, "    .globl {}", symbol.name)?,
        }

        if symbol.alignment > 1 {
            writeln!(f, "    .p2align {}", symbol.alignment.trailing_zeros())?;
        }

        writeln!(f, "{}:", symbol.name)?;

        Ok(())
    }

    fn print_data_object(&self, f: &mut std::fmt::Formatter<'_>, bytes: &[u8]) -> std::fmt::Result {
        if bytes.is_empty() {
            return Ok(());
        }

        write!(f, "    .byte ")?;
        for (i, b) in bytes.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            write!(f, "{b}")?;
        }
        Ok(())
    }
}

impl<T: TargetArch> Display for FunctionPrinter<'_, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let block_names = self.build_block_names();

        for block in &self.func.blocks {
            writeln!(f, "{}:", block_names[&block.id])?;
            for inst in &block.instructions {
                writeln!(
                    f,
                    "    {}",
                    T::InstPrinter::new(inst, &block_names, &self.module.symbol_names,)
                )?;
            }
        }

        Ok(())
    }
}

impl<T: TargetArch> FunctionPrinter<'_, T> {
    fn build_block_names(&self) -> HashMap<BlockId, String> {
        let mut renamer = Renamer::new();
        let function_name = &self.func.name;
        self.func
            .blocks
            .iter()
            .map(|block| {
                (
                    block.id,
                    renamer.intern_name(block.id, format!(".L{}_{}", function_name, block.name)),
                )
            })
            .collect()
    }
}

impl<R> Display for Register<R>
where
    R: Clone + Copy + PartialEq + Eq + Hash + Debug + Display,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Register::Virtual(_) => panic!("virtual reg should be removed!"),
            Register::Physical(phy) => write!(f, "{}", phy),
        }
    }
}
