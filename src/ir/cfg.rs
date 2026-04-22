use std::collections::{HashMap, HashSet};

use enum_as_inner::EnumAsInner;

use crate::ir::{
    core::{BlockId, FunctionId, ModuleCore},
    core_inst::CondBranch,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumAsInner)]
pub(super) enum CFGNode {
    Block(BlockId),
    Fake,
}

impl From<BlockId> for CFGNode {
    fn from(value: BlockId) -> Self {
        CFGNode::Block(value)
    }
}

pub(super) struct DFSResult {
    pub(super) order: Vec<CFGNode>,
    pub(super) dfn: HashMap<CFGNode, usize>,
    pub(super) parent: HashMap<CFGNode, CFGNode>,
}

#[derive(Debug)]
pub(super) struct ControlFlowGraph {
    pub(super) entry: CFGNode,
    pub(super) succs: HashMap<CFGNode, HashSet<CFGNode>>,
    pub(super) preds: HashMap<CFGNode, HashSet<CFGNode>>,
}

pub(super) struct DominatorTree {
    pub(super) root: CFGNode,
    pub(super) children: HashMap<CFGNode, HashSet<CFGNode>>,
    pub(super) idom: HashMap<CFGNode, CFGNode>,
}

impl ControlFlowGraph {
    pub(super) fn reverse(&mut self, ends: HashSet<BlockId>) {
        std::mem::swap(&mut self.succs, &mut self.preds);
        self.entry = CFGNode::Fake;
        self.succs.entry(self.entry).or_default();
        self.preds.entry(self.entry).or_default();
        for end in ends {
            self.succs
                .entry(CFGNode::Fake)
                .or_default()
                .insert(CFGNode::Block(end));
            self.preds
                .entry(CFGNode::Block(end))
                .or_default()
                .insert(CFGNode::Fake);
        }
    }

    pub(super) fn build_dfn(&self) -> DFSResult {
        let mut visited = HashSet::new();
        let mut order = Vec::new();
        let mut parent = HashMap::new();

        fn dfs_visit(
            cfg: &ControlFlowGraph,
            visited: &mut HashSet<CFGNode>,
            order: &mut Vec<CFGNode>,
            parent: &mut HashMap<CFGNode, CFGNode>,
            block: CFGNode,
            from: Option<CFGNode>,
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
        let dfn: HashMap<CFGNode, usize> = order
            .iter()
            .enumerate()
            .map(|(i, block)| (*block, i))
            .collect();

        DFSResult { order, dfn, parent }
    }

    pub(super) fn build_dom_tree(&self) -> DominatorTree {
        let DFSResult {
            order: dfs_order,
            dfn,
            parent: dfs_parent,
        } = self.build_dfn();
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
        let mut bucket: HashMap<CFGNode, HashSet<CFGNode>> = HashMap::new();

        fn find(
            v: CFGNode,
            disjoint_set: &mut HashMap<CFGNode, CFGNode>,
            dfn: &HashMap<CFGNode, usize>,
            sdom: &HashMap<CFGNode, CFGNode>,
            best: &mut HashMap<CFGNode, CFGNode>,
        ) -> CFGNode {
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

        fn link(parent: CFGNode, child: CFGNode, disjoint_set: &mut HashMap<CFGNode, CFGNode>) {
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

        let mut children: HashMap<CFGNode, HashSet<CFGNode>> =
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
    ) -> HashMap<CFGNode, HashSet<CFGNode>> {
        let mut frontier: HashMap<CFGNode, HashSet<CFGNode>> = HashMap::new();

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

impl DominatorTree {
    pub(super) fn dominates(&self, a: CFGNode, b: CFGNode) -> bool {
        if a == self.root {
            return true;
        }

        let mut current = b;
        while current != self.root {
            if current == a {
                return true;
            }
            current = *self.idom.get(&current).unwrap_or_else(|| {
                panic!(
                    "Node {:?} is not reachable from the entry, and entry is {:?}",
                    current, self.root
                )
            });
        }
        false
    }
}

impl ModuleCore {
    pub(super) fn build_cfg(&self, function: FunctionId) -> ControlFlowGraph {
        let mut succs: HashMap<CFGNode, HashSet<CFGNode>> = HashMap::new();
        let mut preds: HashMap<CFGNode, HashSet<CFGNode>> = HashMap::new();

        let func = self.func(function);

        let entry = CFGNode::Block(func.entry);

        for (block_id, data) in &func.blocks {
            let block_id = CFGNode::Block(block_id);
            succs.entry(block_id).or_default();
            preds.entry(block_id).or_default();

            if let Some(term) = data.terminator {
                match func.insts[term].kind {
                    super::core_inst::InstKind::Branch { then_block, cond } => {
                        let then_block = CFGNode::Block(then_block);
                        succs.entry(block_id).or_default().insert(then_block);
                        preds.entry(then_block).or_default().insert(block_id);
                        if let Some(CondBranch { else_block, .. }) = cond {
                            let else_block = CFGNode::Block(else_block);
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
