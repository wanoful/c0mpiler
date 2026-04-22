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
    fn build_dfn(
        &self,
    ) -> (
        Vec<BlockId>,
        HashMap<BlockId, usize>,
        HashMap<BlockId, BlockId>,
    ) {
        let mut visited = HashSet::new();
        let mut order = Vec::new();
        let mut parent = HashMap::new();

        fn dfs_visit(
            cfg: &ControlFlowGraph,
            visited: &mut HashSet<BlockId>,
            order: &mut Vec<BlockId>,
            parent: &mut HashMap<BlockId, BlockId>,
            block: BlockId,
            from: Option<BlockId>,
        ) {
            if visited.contains(&block) {
                return;
            }
            if let Some(p) = from {
                parent.insert(block, p);
            }
            order.push(block);
            visited.insert(block);
            for succ in cfg.succs.get(&block).unwrap_or(&HashSet::new()) {
                dfs_visit(cfg, visited, order, parent, *succ, Some(block));
            }
        }

        dfs_visit(
            self,
            &mut visited,
            &mut order,
            &mut parent,
            self.entry,
            None,
        );
        let dfn: HashMap<BlockId, usize> = order
            .iter()
            .enumerate()
            .map(|(i, block)| (*block, i))
            .collect();

        (order, dfn, parent)
    }

    pub(super) fn build_dom_tree(&self) -> DominatorTree {
        let (dfs_order, dfn, dfs_parent) = self.build_dfn();
        assert!(
            !dfs_order.is_empty(),
            "CFG entry is unreachable from itself"
        );

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

        for &node in dfs_order.iter().rev() {
            if node == self.entry {
                continue;
            }

            for pred in self.preds[&node].iter() {
                if !dfn.contains_key(pred) {
                    continue;
                }
                let candidate = find(*pred, &mut disjoint_set, &dfn, &sdom, &mut best);

                if dfn[&sdom[&candidate]] < dfn[&sdom[&node]] {
                    sdom.insert(node, candidate);
                }
            }

            bucket.entry(sdom[&node]).or_default().insert(node);

            let parent = dfs_parent[&node];
            link(parent, node, &mut disjoint_set);

            for v in bucket.entry(parent).or_default().iter() {
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

        idom.insert(self.entry, self.entry);

        let mut children: HashMap<BlockId, HashSet<BlockId>> =
            idom.keys().map(|&block| (block, HashSet::new())).collect();
        for (&node, &idom_node) in &idom {
            if node != idom_node {
                children.get_mut(&idom_node).unwrap().insert(node);
            }
        }

        DominatorTree {
            root: self.entry,
            children,
            idom,
        }
    }

    pub(super) fn build_dom_frontier(
        &self,
        dom_tree: &DominatorTree,
    ) -> HashMap<BlockId, HashSet<BlockId>> {
        let mut frontier: HashMap<BlockId, HashSet<BlockId>> = HashMap::new();

        for block in dom_tree.idom.keys() {
            if self.preds[block].len() >= 2 {
                for pred in &self.preds[block] {
                    if !dom_tree.idom.contains_key(pred) {
                        continue;
                    }
                    let mut runner = *pred;
                    while runner != dom_tree.idom[block] {
                        frontier.entry(runner).or_default().insert(*block);
                        if let Some(next) = dom_tree.idom.get(&runner)
                            && *next != runner
                        {
                            runner = *next;
                        } else {
                            break;
                        }
                    }
                }
            }
        }

        frontier
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
                        if let Some(CondBranch { else_block, .. }) = cond {
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
