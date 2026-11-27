use crate::inst::Instruction;
use std::fmt::Write;

/// Format a decoded instruction as human-readable RISC-V assembly.
pub fn disassemble(pc: u32, inst: &Instruction) -> String {
    let mut out = String::new();
    match *inst {
        Instruction::Lui { rd, imm } => {
            let _ = write!(out, "lui {rd}, 0x{:x}", (imm as u32) >> 12);
        }
        Instruction::Auipc { rd, imm } => {
            let _ = write!(out, "auipc {rd}, 0x{:x}", (imm as u32) >> 12);
        }
        Instruction::Jal { rd, imm } => {
            let target = pc.wrapping_add(imm as u32);
            let _ = write!(out, "jal {rd}, 0x{target:08x}");
        }
        Instruction::Jalr { rd, rs1, imm } => {
            let _ = write!(out, "jalr {rd}, {imm}({rs1})");
        }
        Instruction::Beq { rs1, rs2, imm } => {
            let target = pc.wrapping_add(imm as u32);
            let _ = write!(out, "beq {rs1}, {rs2}, 0x{target:08x}");
        }
        Instruction::Bne { rs1, rs2, imm } => {
            let target = pc.wrapping_add(imm as u32);
            let _ = write!(out, "bne {rs1}, {rs2}, 0x{target:08x}");
        }
        Instruction::Blt { rs1, rs2, imm } => {
            let target = pc.wrapping_add(imm as u32);
            let _ = write!(out, "blt {rs1}, {rs2}, 0x{target:08x}");
        }
        Instruction::Bge { rs1, rs2, imm } => {
            let target = pc.wrapping_add(imm as u32);
            let _ = write!(out, "bge {rs1}, {rs2}, 0x{target:08x}");
        }
        Instruction::Bltu { rs1, rs2, imm } => {
            let target = pc.wrapping_add(imm as u32);
            let _ = write!(out, "bltu {rs1}, {rs2}, 0x{target:08x}");
        }
        Instruction::Bgeu { rs1, rs2, imm } => {
            let target = pc.wrapping_add(imm as u32);
            let _ = write!(out, "bgeu {rs1}, {rs2}, 0x{target:08x}");
        }
        Instruction::Lb { rd, rs1, imm } => {
            let _ = write!(out, "lb {rd}, {imm}({rs1})");
        }
        Instruction::Lh { rd, rs1, imm } => {
            let _ = write!(out, "lh {rd}, {imm}({rs1})");
        }
        Instruction::Lw { rd, rs1, imm } => {
            let _ = write!(out, "lw {rd}, {imm}({rs1})");
        }
        Instruction::Lbu { rd, rs1, imm } => {
            let _ = write!(out, "lbu {rd}, {imm}({rs1})");
        }
        Instruction::Lhu { rd, rs1, imm } => {
            let _ = write!(out, "lhu {rd}, {imm}({rs1})");
        }
        Instruction::Sb { rs1, rs2, imm } => {
            let _ = write!(out, "sb {rs2}, {imm}({rs1})");
        }
        Instruction::Sh { rs1, rs2, imm } => {
            let _ = write!(out, "sh {rs2}, {imm}({rs1})");
        }
        Instruction::Sw { rs1, rs2, imm } => {
            let _ = write!(out, "sw {rs2}, {imm}({rs1})");
        }
        Instruction::Addi { rd, rs1, imm } => {
            let _ = write!(out, "addi {rd}, {rs1}, {imm}");
        }
        Instruction::Slti { rd, rs1, imm } => {
            let _ = write!(out, "slti {rd}, {rs1}, {imm}");
        }
        Instruction::Sltiu { rd, rs1, imm } => {
            let _ = write!(out, "sltiu {rd}, {rs1}, {imm}");
        }
        Instruction::Xori { rd, rs1, imm } => {
            let _ = write!(out, "xori {rd}, {rs1}, {imm}");
        }
        Instruction::Ori { rd, rs1, imm } => {
            let _ = write!(out, "ori {rd}, {rs1}, {imm}");
        }
        Instruction::Andi { rd, rs1, imm } => {
            let _ = write!(out, "andi {rd}, {rs1}, {imm}");
        }
        Instruction::Slli { rd, rs1, shamt } => {
            let _ = write!(out, "slli {rd}, {rs1}, {shamt}");
        }
        Instruction::Srli { rd, rs1, shamt } => {
            let _ = write!(out, "srli {rd}, {rs1}, {shamt}");
        }
        Instruction::Srai { rd, rs1, shamt } => {
            let _ = write!(out, "srai {rd}, {rs1}, {shamt}");
        }
        Instruction::Add { rd, rs1, rs2 } => {
            let _ = write!(out, "add {rd}, {rs1}, {rs2}");
        }
        Instruction::Sub { rd, rs1, rs2 } => {
            let _ = write!(out, "sub {rd}, {rs1}, {rs2}");
        }
        Instruction::Sll { rd, rs1, rs2 } => {
            let _ = write!(out, "sll {rd}, {rs1}, {rs2}");
        }
        Instruction::Slt { rd, rs1, rs2 } => {
            let _ = write!(out, "slt {rd}, {rs1}, {rs2}");
        }
        Instruction::Sltu { rd, rs1, rs2 } => {
            let _ = write!(out, "sltu {rd}, {rs1}, {rs2}");
        }
        Instruction::Xor { rd, rs1, rs2 } => {
            let _ = write!(out, "xor {rd}, {rs1}, {rs2}");
        }
        Instruction::Srl { rd, rs1, rs2 } => {
            let _ = write!(out, "srl {rd}, {rs1}, {rs2}");
        }
        Instruction::Sra { rd, rs1, rs2 } => {
            let _ = write!(out, "sra {rd}, {rs1}, {rs2}");
        }
        Instruction::Or { rd, rs1, rs2 } => {
            let _ = write!(out, "or {rd}, {rs1}, {rs2}");
        }
        Instruction::And { rd, rs1, rs2 } => {
            let _ = write!(out, "and {rd}, {rs1}, {rs2}");
        }
        Instruction::Mul { rd, rs1, rs2 } => {
            let _ = write!(out, "mul {rd}, {rs1}, {rs2}");
        }
        Instruction::Mulh { rd, rs1, rs2 } => {
            let _ = write!(out, "mulh {rd}, {rs1}, {rs2}");
        }
        Instruction::Mulhsu { rd, rs1, rs2 } => {
            let _ = write!(out, "mulhsu {rd}, {rs1}, {rs2}");
        }
        Instruction::Mulhu { rd, rs1, rs2 } => {
            let _ = write!(out, "mulhu {rd}, {rs1}, {rs2}");
        }
        Instruction::Div { rd, rs1, rs2 } => {
            let _ = write!(out, "div {rd}, {rs1}, {rs2}");
        }
        Instruction::Divu { rd, rs1, rs2 } => {
            let _ = write!(out, "divu {rd}, {rs1}, {rs2}");
        }
        Instruction::Rem { rd, rs1, rs2 } => {
            let _ = write!(out, "rem {rd}, {rs1}, {rs2}");
        }
        Instruction::Remu { rd, rs1, rs2 } => {
            let _ = write!(out, "remu {rd}, {rs1}, {rs2}");
        }
        Instruction::Flw { rd, rs1, imm } => {
            let _ = write!(out, "flw {rd}, {imm}({rs1})");
        }
        Instruction::Fsw { rs1, rs2, imm } => {
            let _ = write!(out, "fsw {rs2}, {imm}({rs1})");
        }
        Instruction::FmaddS { rd, rs1, rs2, rs3, .. } => {
            let _ = write!(out, "fmadd.s {rd}, {rs1}, {rs2}, {rs3}");
        }
        Instruction::FmsubS { rd, rs1, rs2, rs3, .. } => {
            let _ = write!(out, "fmsub.s {rd}, {rs1}, {rs2}, {rs3}");
        }
        Instruction::FnmsubS { rd, rs1, rs2, rs3, .. } => {
            let _ = write!(out, "fnmsub.s {rd}, {rs1}, {rs2}, {rs3}");
        }
        Instruction::FnmaddS { rd, rs1, rs2, rs3, .. } => {
            let _ = write!(out, "fnmadd.s {rd}, {rs1}, {rs2}, {rs3}");
        }
        Instruction::FaddS { rd, rs1, rs2, .. } => {
            let _ = write!(out, "fadd.s {rd}, {rs1}, {rs2}");
        }
        Instruction::FsubS { rd, rs1, rs2, .. } => {
            let _ = write!(out, "fsub.s {rd}, {rs1}, {rs2}");
        }
        Instruction::FmulS { rd, rs1, rs2, .. } => {
            let _ = write!(out, "fmul.s {rd}, {rs1}, {rs2}");
        }
        Instruction::FdivS { rd, rs1, rs2, .. } => {
            let _ = write!(out, "fdiv.s {rd}, {rs1}, {rs2}");
        }
        Instruction::FsqrtS { rd, rs1, .. } => {
            let _ = write!(out, "fsqrt.s {rd}, {rs1}");
        }
        Instruction::FsgnjS { rd, rs1, rs2 } => {
            let _ = write!(out, "fsgnj.s {rd}, {rs1}, {rs2}");
        }
        Instruction::FsgnjnS { rd, rs1, rs2 } => {
            let _ = write!(out, "fsgnjn.s {rd}, {rs1}, {rs2}");
        }
        Instruction::FsgnjxS { rd, rs1, rs2 } => {
            let _ = write!(out, "fsgnjx.s {rd}, {rs1}, {rs2}");
        }
        Instruction::FminS { rd, rs1, rs2 } => {
            let _ = write!(out, "fmin.s {rd}, {rs1}, {rs2}");
        }
        Instruction::FmaxS { rd, rs1, rs2 } => {
            let _ = write!(out, "fmax.s {rd}, {rs1}, {rs2}");
        }
        Instruction::FcvtWS { rd, rs1, .. } => {
            let _ = write!(out, "fcvt.w.s {rd}, {rs1}");
        }
        Instruction::FcvtWuS { rd, rs1, .. } => {
            let _ = write!(out, "fcvt.wu.s {rd}, {rs1}");
        }
        Instruction::FmvXW { rd, rs1 } => {
            let _ = write!(out, "fmv.x.w {rd}, {rs1}");
        }
        Instruction::FeqS { rd, rs1, rs2 } => {
            let _ = write!(out, "feq.s {rd}, {rs1}, {rs2}");
        }
        Instruction::FltS { rd, rs1, rs2 } => {
            let _ = write!(out, "flt.s {rd}, {rs1}, {rs2}");
        }
        Instruction::FleS { rd, rs1, rs2 } => {
            let _ = write!(out, "fle.s {rd}, {rs1}, {rs2}");
        }
        Instruction::FclassS { rd, rs1 } => {
            let _ = write!(out, "fclass.s {rd}, {rs1}");
        }
        Instruction::FcvtSW { rd, rs1, .. } => {
            let _ = write!(out, "fcvt.s.w {rd}, {rs1}");
        }
        Instruction::FcvtSWu { rd, rs1, .. } => {
            let _ = write!(out, "fcvt.s.wu {rd}, {rs1}");
        }
        Instruction::FmvWX { rd, rs1 } => {
            let _ = write!(out, "fmv.w.x {rd}, {rs1}");
        }
        Instruction::Csrrw { rd, rs1, csr } => {
            let _ = write!(out, "csrrw {rd}, 0x{csr:03x}, {rs1}");
        }
        Instruction::Csrrs { rd, rs1, csr } => {
            let _ = write!(out, "csrrs {rd}, 0x{csr:03x}, {rs1}");
        }
        Instruction::Csrrc { rd, rs1, csr } => {
            let _ = write!(out, "csrrc {rd}, 0x{csr:03x}, {rs1}");
        }
        Instruction::Csrrwi { rd, zimm, csr } => {
            let _ = write!(out, "csrrwi {rd}, 0x{csr:03x}, {zimm}");
        }
        Instruction::Csrrsi { rd, zimm, csr } => {
            let _ = write!(out, "csrrsi {rd}, 0x{csr:03x}, {zimm}");
        }
        Instruction::Csrrci { rd, zimm, csr } => {
            let _ = write!(out, "csrrci {rd}, 0x{csr:03x}, {zimm}");
        }
        Instruction::Fence => out.push_str("fence"),
        Instruction::Ecall => out.push_str("ecall"),
        Instruction::Ebreak => out.push_str("ebreak"),
    }
    out
}
