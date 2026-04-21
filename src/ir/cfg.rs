use std::collections::{HashMap, HashSet};

use crate::ir::{
    core::{BlockId, FunctionId, ModuleCore},
    core_inst::CondBranch,
};

pub(super) struct ControlFlowGraph {
    pub(super) entry: BlockId,
    pub(super) succs: HashMap<BlockId, HashSet<BlockId>>,
    pub(super) preds: HashMap<BlockId, HashSet<BlockId>>,
}

pub(super) struct DominatorTree {
    pub(super) root: BlockId,
    pub(super) children: HashMap<BlockId, HashSet<BlockId>>,
    pub(super) idom: HashMap<BlockId, BlockId>,
}

impl ControlFlowGraph {
    fn build_dfn(&self) -> (Vec<BlockId>, HashMap<BlockId, usize>) {
        let mut visited = HashSet::new();
        let mut order = Vec::new();

        fn dfs_visit(
            cfg: &ControlFlowGraph,
            visited: &mut HashSet<BlockId>,
            order: &mut Vec<BlockId>,
            block: BlockId,
        ) {
            if visited.contains(&block) {
                return;
            }
            order.push(block);
            visited.insert(block);
            for succ in cfg.succs.get(&block).unwrap_or(&HashSet::new()) {
                dfs_visit(cfg, visited, order, *succ);
            }
        }

        dfs_visit(self, &mut visited, &mut order, self.entry);
        let dfn = order
            .iter()
            .enumerate()
            .map(|(i, block)| (*block, i))
            .collect();
        (order, dfn)
    }

    pub(super) fn build_dom_tree(&self) -> DominatorTree {
        let (dfs_order, dfn) = self.build_dfn();
        let mut sdom = dfs_order
            .iter()
            .map(|&block| (block, block))
            .collect::<HashMap<_, _>>();
        let mut idom = sdom.clone();
        let mut disjoint_set = sdom.clone();
        let mut best = sdom.clone();
        let mut bucket: HashMap<BlockId, HashSet<BlockId>> = HashMap::new();

        fn find(
            v: BlockId,
            disjoint_set: &mut HashMap<BlockId, BlockId>,
            dfn: &HashMap<BlockId, usize>,
            sdom: &HashMap<BlockId, BlockId>,
            best: &mut HashMap<BlockId, BlockId>,
        ) -> BlockId {
            let parent = disjoint_set[&v];
            if parent != v {
                let root = find(parent, disjoint_set, dfn, sdom, best);
                if dfn[&sdom[&best[&parent]]] < dfn[&sdom[&best[&v]]] {
                    best.insert(v, best[&parent]);
                }

                disjoint_set.insert(v, root);
                root
            } else {
                v
            }
        }

        fn link(parent: BlockId, child: BlockId, disjoint_set: &mut HashMap<BlockId, BlockId>) {
            disjoint_set.insert(child, parent);
        }

        for node in dfs_order.iter().rev() {
            for pred in self.preds[node].iter() {
                let candidate = find(*pred, &mut disjoint_set, &dfn, &sdom, &mut best);

                if dfn[&sdom[&candidate]] < dfn[&sdom[&node]] {
                    sdom.insert(*node, candidate);
                }
            }

            bucket.entry(sdom[node]).or_default().insert(*node);

            let parent = disjoint_set[node];
            link(parent, *node, &mut disjoint_set);

            for v in bucket[&parent].iter() {
                let w = find(*v, &mut disjoint_set, &dfn, &sdom, &mut best);
                if sdom[&w] == sdom[v] {
                    idom.insert(*v, sdom[v]);
                } else {
                    idom.insert(*v, w);
                }
            }
            bucket.remove(&parent);
        }

        for node in dfs_order.iter().skip(1) {
            if idom[node] != sdom[node] {
                idom.insert(*node, idom[&idom[node]]);
            }
        }

        *idom.get_mut(&self.entry).unwrap() = self.entry;

        let mut children: HashMap<BlockId, HashSet<BlockId>> = HashMap::new();
        for (&node, &idom_node) in &idom {
            if node != idom_node {
                children.entry(idom_node).or_default().insert(node);
            }
        }

        DominatorTree {
            root: self.entry,
            children,
            idom,
        }
    }
}

impl ModuleCore {
    pub(super) fn build_cfg(&self, function: FunctionId) -> ControlFlowGraph {
        let mut succs: HashMap<BlockId, HashSet<BlockId>> = HashMap::new();
        let mut preds: HashMap<BlockId, HashSet<BlockId>> = HashMap::new();

        let func = self.func(function);

        let entry = func.entry;

        for (block_id, data) in &func.blocks {
            succs.entry(block_id).or_default();
            preds.entry(block_id).or_default();

            if let Some(term) = data.terminator {
                match func.insts[term].kind {
                    super::core_inst::InstKind::Branch { then_block, cond } => {
                        succs.entry(block_id).or_default().insert(then_block);
                        preds.entry(then_block).or_default().insert(block_id);
                        if let Some(CondBranch { cond, else_block }) = cond {
                            succs.entry(block_id).or_default().insert(else_block);
                            preds.entry(else_block).or_default().insert(block_id);
                        }
                    }
                    super::core_inst::InstKind::Ret { .. } => {}
                    super::core_inst::InstKind::Unreachable => {}
                    _ => {}
                }
            }
        }

        ControlFlowGraph {
            entry,
            succs,
            preds,
        }
    }
}
