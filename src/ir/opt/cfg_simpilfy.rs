use crate::ir::core::{FunctionId, ModuleCore};

impl ModuleCore {
    pub fn opt_cfg_simplify(&mut self) {
        for id in self.functions_in_order() {
            self.func_cfg_simplify(id);
        }
    }

    fn func_cfg_simplify(&mut self, id: FunctionId) {
        let function = self.func(id);
        if function.is_declare {
            return;
        }

        let mut changed = true;
        while changed {
            changed = false;
        }
    }
}