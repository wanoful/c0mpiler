use crate::ir::core::{BlockId, BlockRef, FunctionId, InstId, InstRef, ModuleCore};

#[derive(Debug, Clone, Copy)]
pub enum InsertPoint {
    AppendPhi(BlockRef),
    BeforeFirstNonPhi(BlockRef),
    BeforeTerminator(BlockRef),
    Before(InstRef),
    After(InstRef),
}

pub struct IRBuilder<'a> {
    pub module: &'a mut ModuleCore,
    pub insert_point: InsertPoint,
}

impl<'a> IRBuilder<'a> {
    pub fn locate_append_phi(&mut self, block: BlockRef) {
        self.insert_point = InsertPoint::AppendPhi(block);
    }

    pub fn locate_before_first_non_phi(&mut self, block: BlockRef) {
        self.insert_point = InsertPoint::BeforeFirstNonPhi(block);
    }

    pub fn locate_before_terminator(&mut self, block: BlockRef) {
        self.insert_point = InsertPoint::BeforeTerminator(block);
    }

    pub fn locate_before(&mut self, inst: InstRef) {
        self.insert_point = InsertPoint::Before(inst);
    }

    pub fn locate_after(&mut self, inst: InstRef) {
        self.insert_point = InsertPoint::After(inst);
    }

    fn insert_inst(&mut self, inst: InstRef) {
        match self.insert_point {
            InsertPoint::AppendPhi(block_ref) => {
                self.module.append_phi(block_ref, inst);
            }
            InsertPoint::BeforeFirstNonPhi(block_ref) => {
                self.module.push_front_inst(block_ref, inst);
            }
            InsertPoint::BeforeTerminator(block_ref) => {
                self.module.append_inst(block_ref, inst);
            }
            InsertPoint::Before(inst_ref) => {
                self.module.insert_before(inst_ref, inst);
            }
            InsertPoint::After(inst_ref) => {
                self.module.insert_after(inst_ref, inst);
            }
        }
    }
}
