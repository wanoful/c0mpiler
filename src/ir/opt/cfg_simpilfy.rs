use std::collections::HashSet;

use crate::ir::{
    cfg::DFSResult,
    core::{FunctionId, ModuleCore},
};

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

        self.dead_block_elimination(id);
    }

    fn dead_block_elimination(&mut self, id: FunctionId) {
        let cfg = self.build_cfg(id);
        let DFSResult {
            order: dfs_order, ..
        } = cfg.build_dfn();
        let reachable_blocks = dfs_order
            .into_iter()
            .filter_map(|node| node.as_block().cloned())
            .collect::<HashSet<_>>();

        let dead_blocks = self
            .func(id)
            .block_order
            .iter()
            .filter_map(|block_id| {
                (!reachable_blocks.contains(block_id)).then(|| crate::ir::core::BlockRef {
                    func: id,
                    block: *block_id,
                })
            })
            .collect::<Vec<_>>();

        self.erase_blocks_from_parent(dead_blocks);
    }
}
