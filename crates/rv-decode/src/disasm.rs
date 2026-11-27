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
        Instruction::Fence => out.push_str("fence"),
        Instruction::Ecall => out.push_str("ecall"),
        Instruction::Ebreak => out.push_str("ebreak"),
    }
    out
}
