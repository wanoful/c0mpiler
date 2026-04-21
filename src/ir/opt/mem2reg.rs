use crate::ir::core::{FunctionId, ModuleCore};

impl ModuleCore {
    pub fn opt_pass_mem2reg(&mut self) {
        for id in self.functions_in_order() {
            self.func_mem2reg(id);
        }
    }

    fn func_mem2reg(&mut self, id: FunctionId) {
        let function = self.func_mut(id);
        if function.is_declare {
            return;
        }

        let cfg = self.build_cfg(id);
        let dom_tree = cfg.build_dom_tree();

        
        

        
    }
}

