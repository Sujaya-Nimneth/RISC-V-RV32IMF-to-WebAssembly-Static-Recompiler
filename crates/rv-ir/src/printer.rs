use crate::ir::{IrFunction, IrOp, IrTerm};
use std::fmt::Write;

/// Render an `IrFunction` in a deterministic, human-readable SSA IR format.
pub fn print_function(func: &IrFunction) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "fn {} @ 0x{:08x} {{", func.name, func.entry);
    for bb in &func.blocks {
        let _ = writeln!(out, "  bb{} (0x{:08x}):", bb.id.0, bb.guest_pc);
        for phi in &bb.phis {
            let _ = write!(out, "    v{} = phi {:?} [", phi.dst.0, phi.ty);
            for (i, (pred, val)) in phi.incoming.iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                let _ = write!(out, "bb{}: v{}", pred.0, val.0);
            }
            out.push_str("]\n");
        }
        for inst in &bb.insts {
            out.push_str("    ");
            if let Some(dst) = inst.dst {
                let _ = write!(out, "v{} = ", dst.0);
            }
            match &inst.op {
                IrOp::ConstI32(c) => {
                    let _ = write!(out, "const.i32 {c}");
                }
                IrOp::ConstF32(bits) => {
                    let _ = write!(out, "const.f32 0x{bits:08x}");
                }
                IrOp::Copy(v) => {
                    let _ = write!(out, "copy v{}", v.0);
                }
                IrOp::ReadReg(r) => {
                    let _ = write!(out, "read_reg {r}");
                }
                IrOp::WriteReg { reg, val } => {
                    let _ = write!(out, "write_reg {reg}, v{}", val.0);
                }
                IrOp::ReadFReg(r) => {
                    let _ = write!(out, "read_freg {r}");
                }
                IrOp::WriteFReg { freg, val } => {
                    let _ = write!(out, "write_freg {freg}, v{}", val.0);
                }
                IrOp::BinOp { op, lhs, rhs } => {
                    let _ = write!(out, "{op:?} v{}, v{}", lhs.0, rhs.0);
                }
                IrOp::Cmp { op, lhs, rhs } => {
                    let _ = write!(out, "cmp.{op:?} v{}, v{}", lhs.0, rhs.0);
                }
                IrOp::Load {
                    addr,
                    offset,
                    width,
                    signed,
                } => {
                    let s = if *signed { "s" } else { "u" };
                    let _ = write!(out, "load.{width:?}.{s} [v{} + {offset}]", addr.0);
                }
                IrOp::Store {
                    addr,
                    val,
                    offset,
                    width,
                } => {
                    let _ = write!(out, "store.{width:?} [v{} + {offset}], v{}", addr.0, val.0);
                }
                IrOp::Ecall => out.push_str("ecall"),
                IrOp::AddInstret(n) => {
                    let _ = write!(out, "instret += {n}");
                }
            }
            out.push('\n');
        }
        out.push_str("    ");
        match &bb.term {
            IrTerm::Jump { target } => {
                let _ = writeln!(out, "br bb{}", target.0);
            }
            IrTerm::BrIf {
                cond,
                then_bb,
                else_bb,
            } => {
                let _ = writeln!(out, "br_if v{}, bb{}, bb{}", cond.0, then_bb.0, else_bb.0);
            }
            IrTerm::BrIfCmp {
                op,
                lhs,
                rhs,
                then_bb,
                else_bb,
            } => {
                let _ = writeln!(
                    out,
                    "br_if.{op:?} v{}, v{}, bb{}, bb{}",
                    lhs.0, rhs.0, then_bb.0, else_bb.0
                );
            }
            IrTerm::Call {
                target_pc,
                fallthrough,
                ..
            } => {
                let _ = writeln!(out, "call 0x{target_pc:08x} -> bb{}", fallthrough.0);
            }
            IrTerm::TailCall { target_pc } => {
                let _ = writeln!(out, "tail_call 0x{target_pc:08x}");
            }
            IrTerm::IndirectCall {
                target_val,
                fallthrough,
                ..
            } => {
                let _ = writeln!(out, "call_indirect v{} -> bb{}", target_val.0, fallthrough.0);
            }
            IrTerm::IndirectJump { target_val } => {
                let _ = writeln!(out, "jump_indirect v{}", target_val.0);
            }
            IrTerm::Return { target_val } => {
                let _ = writeln!(out, "ret v{}", target_val.0);
            }
            IrTerm::Trap => out.push_str("trap\n"),
        }
    }
    out.push_str("}\n");
    out
}
