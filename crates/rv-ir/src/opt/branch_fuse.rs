use crate::ir::{CmpOp, IrFunction, IrOp, IrTerm, ValueId};
use std::collections::BTreeMap;

/// Fuse `Cmp` + `BrIf` in the same basic block into `BrIfCmp`.
pub fn run(func: &mut IrFunction) {
    for bb in &mut func.blocks {
        let IrTerm::BrIf {
            cond,
            then_bb,
            else_bb,
        } = bb.term
        else {
            continue;
        };

        let mut cmps: BTreeMap<ValueId, (CmpOp, ValueId, ValueId)> = BTreeMap::new();
        for inst in &bb.insts {
            if let (Some(dst), IrOp::Cmp { op, lhs, rhs }) = (inst.dst, &inst.op) {
                cmps.insert(dst, (*op, *lhs, *rhs));
            }
        }

        if let Some(&(op, lhs, rhs)) = cmps.get(&cond) {
            bb.term = IrTerm::BrIfCmp {
                op,
                lhs,
                rhs,
                then_bb,
                else_bb,
            };
        }
    }
}
