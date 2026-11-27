use crate::ir::{IrFunction, IrOp};
use rv_decode::Reg;

/// Eliminate reads/writes of `x0` (`Reg::ZERO`).
pub fn run(func: &mut IrFunction) {
    for bb in &mut func.blocks {
        bb.insts.retain_mut(|inst| match &mut inst.op {
            IrOp::ReadReg(r) if *r == Reg::ZERO => {
                inst.op = IrOp::ConstI32(0);
                true
            }
            IrOp::WriteReg { reg, .. } if *reg == Reg::ZERO => false,
            _ => true,
        });
    }
}
