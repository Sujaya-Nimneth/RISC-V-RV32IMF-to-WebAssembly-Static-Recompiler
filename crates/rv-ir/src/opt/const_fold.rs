use crate::ir::{BinOp, CmpOp, IrFunction, IrOp, ValueId};
use crate::opt::reg_promote::{remap_op, remap_term};
use std::collections::BTreeMap;

/// Constant folding, copy propagation, and algebraic identities.
pub fn run(func: &mut IrFunction) {
    let mut changed = true;
    while changed {
        changed = false;
        let mut consts: BTreeMap<ValueId, i32> = BTreeMap::new();
        let mut copies: BTreeMap<ValueId, ValueId> = BTreeMap::new();

        for bb in &mut func.blocks {
            for inst in &mut bb.insts {
                remap_op(&mut inst.op, &|v| copies.get(&v).copied().unwrap_or(v));

                let Some(dst) = inst.dst else {
                    continue;
                };
                match inst.op {
                    IrOp::ConstI32(c) => {
                        consts.insert(dst, c);
                    }
                    IrOp::Copy(src) => {
                        let root = copies.get(&src).copied().unwrap_or(src);
                        copies.insert(dst, root);
                        if let Some(&c) = consts.get(&root) {
                            consts.insert(dst, c);
                        }
                    }
                    IrOp::BinOp { op, lhs, rhs } => {
                        if let (Some(&a), Some(&b)) = (consts.get(&lhs), consts.get(&rhs)) {
                            let folded = eval_binop(op, a, b);
                            inst.op = IrOp::ConstI32(folded);
                            consts.insert(dst, folded);
                            changed = true;
                        } else if let Some(&b) = consts.get(&rhs) {
                            if let Some(new_op) = simplify_rhs(op, lhs, b) {
                                inst.op = new_op;
                                changed = true;
                            }
                        } else if let Some(&a) = consts.get(&lhs) {
                            if let Some(new_op) = simplify_lhs(op, a, rhs) {
                                inst.op = new_op;
                                changed = true;
                            }
                        } else if lhs == rhs {
                            match op {
                                BinOp::Sub | BinOp::Xor => {
                                    inst.op = IrOp::ConstI32(0);
                                    consts.insert(dst, 0);
                                    changed = true;
                                }
                                BinOp::And | BinOp::Or => {
                                    inst.op = IrOp::Copy(lhs);
                                    copies.insert(dst, lhs);
                                    changed = true;
                                }
                                _ => {}
                            }
                        }
                    }
                    IrOp::Cmp { op, lhs, rhs } => {
                        if let (Some(&a), Some(&b)) = (consts.get(&lhs), consts.get(&rhs)) {
                            let res = eval_cmp(op, a, b) as i32;
                            inst.op = IrOp::ConstI32(res);
                            consts.insert(dst, res);
                            changed = true;
                        }
                    }
                    _ => {}
                }
            }
            remap_term(&mut bb.term, &|v| copies.get(&v).copied().unwrap_or(v));
        }

        if !copies.is_empty() {
            for bb in &mut func.blocks {
                for phi in &mut bb.phis {
                    for (_, v) in &mut phi.incoming {
                        if let Some(&root) = copies.get(v) {
                            *v = root;
                            changed = true;
                        }
                    }
                }
                for inst in &mut bb.insts {
                    remap_op(&mut inst.op, &|v| copies.get(&v).copied().unwrap_or(v));
                }
                remap_term(&mut bb.term, &|v| copies.get(&v).copied().unwrap_or(v));
            }
        }
    }
}

fn eval_binop(op: BinOp, a: i32, b: i32) -> i32 {
    let au = a as u32;
    let bu = b as u32;
    match op {
        BinOp::Add => a.wrapping_add(b),
        BinOp::Sub => a.wrapping_sub(b),
        BinOp::Mul => a.wrapping_mul(b),
        BinOp::Mulh => (((a as i64).wrapping_mul(b as i64)) >> 32) as i32,
        BinOp::Mulhsu => (((a as i64 as u64).wrapping_mul(bu as u64)) >> 32) as i32,
        BinOp::Mulhu => (((au as u64).wrapping_mul(bu as u64)) >> 32) as i32,
        BinOp::DivS => {
            if b == 0 {
                -1
            } else if a == i32::MIN && b == -1 {
                i32::MIN
            } else {
                a.wrapping_div(b)
            }
        }
        BinOp::DivU => {
            if bu == 0 {
                -1
            } else {
                (au / bu) as i32
            }
        }
        BinOp::RemS => {
            if b == 0 {
                a
            } else if a == i32::MIN && b == -1 {
                0
            } else {
                a.wrapping_rem(b)
            }
        }
        BinOp::RemU => {
            if bu == 0 {
                a
            } else {
                (au % bu) as i32
            }
        }
        BinOp::And => a & b,
        BinOp::Or => a | b,
        BinOp::Xor => a ^ b,
        BinOp::Shl => au.wrapping_shl(bu & 0x1F) as i32,
        BinOp::ShrU => au.wrapping_shr(bu & 0x1F) as i32,
        BinOp::ShrS => a.wrapping_shr(bu & 0x1F),
    }
}

fn eval_cmp(op: CmpOp, a: i32, b: i32) -> bool {
    match op {
        CmpOp::Eq => a == b,
        CmpOp::Ne => a != b,
        CmpOp::LtS => a < b,
        CmpOp::GeS => a >= b,
        CmpOp::LtU => (a as u32) < (b as u32),
        CmpOp::GeU => (a as u32) >= (b as u32),
    }
}

fn simplify_rhs(op: BinOp, lhs: ValueId, b: i32) -> Option<IrOp> {
    match (op, b) {
        (BinOp::Add | BinOp::Sub | BinOp::Or | BinOp::Xor, 0) => Some(IrOp::Copy(lhs)),
        (BinOp::Shl | BinOp::ShrU | BinOp::ShrS, 0) => Some(IrOp::Copy(lhs)),
        (BinOp::Mul, 1) => Some(IrOp::Copy(lhs)),
        (BinOp::Mul | BinOp::And, 0) => Some(IrOp::ConstI32(0)),
        (BinOp::And | BinOp::Or, -1) => Some(if op == BinOp::And {
            IrOp::Copy(lhs)
        } else {
            IrOp::ConstI32(-1)
        }),
        _ => None,
    }
}

fn simplify_lhs(op: BinOp, a: i32, rhs: ValueId) -> Option<IrOp> {
    match (op, a) {
        (BinOp::Add | BinOp::Or | BinOp::Xor, 0) => Some(IrOp::Copy(rhs)),
        (BinOp::Mul, 1) => Some(IrOp::Copy(rhs)),
        (BinOp::Mul | BinOp::And, 0) => Some(IrOp::ConstI32(0)),
        _ => None,
    }
}
