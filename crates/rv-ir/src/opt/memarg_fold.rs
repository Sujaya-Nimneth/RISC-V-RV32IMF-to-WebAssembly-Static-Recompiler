use crate::ir::{BinOp, IrFunction, IrOp, ValueId};
use std::collections::BTreeMap;

/// Fold non-negative constant address displacements (`base + imm`) into `Load`/`Store`
/// `memarg.offset` immediates. Negative offsets are kept as explicit `Add` + `offset=0`
/// to avoid Wasm 32-bit unsigned address wraparound traps.
pub fn run(func: &mut IrFunction) {
    for bb in &mut func.blocks {
        let mut consts: BTreeMap<ValueId, i32> = BTreeMap::new();
        let mut adds: BTreeMap<ValueId, (ValueId, i32)> = BTreeMap::new();

        for inst in &mut bb.insts {
            if let Some(dst) = inst.dst {
                match inst.op {
                    IrOp::ConstI32(c) => {
                        consts.insert(dst, c);
                    }
                    IrOp::BinOp {
                        op: BinOp::Add,
                        lhs,
                        rhs,
                    } => {
                        if let Some(&c) = consts.get(&rhs) {
                            adds.insert(dst, (lhs, c));
                        } else if let Some(&c) = consts.get(&lhs) {
                            adds.insert(dst, (rhs, c));
                        }
                    }
                    _ => {}
                }
            }

            match &mut inst.op {
                IrOp::Load { addr, offset, .. } | IrOp::Store { addr, offset, .. } => {
                    if let Some(&(base, imm)) = adds.get(addr) {
                        if imm >= 0 {
                            let combined = (*offset as i64) + (imm as i64);
                            if (0..0x0010_0000).contains(&combined) {
                                *addr = base;
                                *offset = combined as i32;
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }
}
