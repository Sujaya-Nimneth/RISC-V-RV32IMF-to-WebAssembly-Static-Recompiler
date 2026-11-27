use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

/// General-purpose RISC-V integer register (`x0`..`x31`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Reg(pub u8);

impl Reg {
    pub const ZERO: Self = Self(0);
    pub const RA: Self = Self(1);
    pub const SP: Self = Self(2);
    pub const GP: Self = Self(3);
    pub const TP: Self = Self(4);
    pub const A0: Self = Self(10);
    pub const A1: Self = Self(11);
    pub const A2: Self = Self(12);
    pub const A7: Self = Self(17);

    #[inline]
    pub fn index(self) -> usize {
        (self.0 & 0x1F) as usize
    }

    pub fn abi_name(self) -> &'static str {
        const NAMES: [&str; 32] = [
            "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1", "a0", "a1", "a2", "a3",
            "a4", "a5", "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11",
            "t3", "t4", "t5", "t6",
        ];
        NAMES[self.index()]
    }
}

impl fmt::Display for Reg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.abi_name())
    }
}

/// RISC-V floating-point register (`f0`..`f31`), used in RV32F.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct FReg(pub u8);

impl FReg {
    #[inline]
    pub fn index(self) -> usize {
        (self.0 & 0x1F) as usize
    }

    pub fn abi_name(self) -> &'static str {
        const NAMES: [&str; 32] = [
            "ft0", "ft1", "ft2", "ft3", "ft4", "ft5", "ft6", "ft7", "fs0", "fs1", "fa0", "fa1",
            "fa2", "fa3", "fa4", "fa5", "fa6", "fa7", "fs2", "fs3", "fs4", "fs5", "fs6", "fs7",
            "fs8", "fs9", "fs10", "fs11", "ft8", "ft9", "ft10", "ft11",
        ];
        NAMES[self.index()]
    }
}

impl fmt::Display for FReg {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.abi_name())
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum DecodeError {
    #[error("16-bit compressed (RVC) instruction at 0x{pc:08x} (0x{word:08x}) is not supported in v1")]
    CompressedUnsupported { pc: u32, word: u32 },
    #[error("invalid or unsupported instruction at 0x{pc:08x}: 0x{word:08x}")]
    UnsupportedInstruction { pc: u32, word: u32 },
}

/// Decoded RV32IM instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Instruction {
    Lui { rd: Reg, imm: i32 },
    Auipc { rd: Reg, imm: i32 },
    Jal { rd: Reg, imm: i32 },
    Jalr { rd: Reg, rs1: Reg, imm: i32 },

    Beq { rs1: Reg, rs2: Reg, imm: i32 },
    Bne { rs1: Reg, rs2: Reg, imm: i32 },
    Blt { rs1: Reg, rs2: Reg, imm: i32 },
    Bge { rs1: Reg, rs2: Reg, imm: i32 },
    Bltu { rs1: Reg, rs2: Reg, imm: i32 },
    Bgeu { rs1: Reg, rs2: Reg, imm: i32 },

    Lb { rd: Reg, rs1: Reg, imm: i32 },
    Lh { rd: Reg, rs1: Reg, imm: i32 },
    Lw { rd: Reg, rs1: Reg, imm: i32 },
    Lbu { rd: Reg, rs1: Reg, imm: i32 },
    Lhu { rd: Reg, rs1: Reg, imm: i32 },

    Sb { rs1: Reg, rs2: Reg, imm: i32 },
    Sh { rs1: Reg, rs2: Reg, imm: i32 },
    Sw { rs1: Reg, rs2: Reg, imm: i32 },

    Addi { rd: Reg, rs1: Reg, imm: i32 },
    Slti { rd: Reg, rs1: Reg, imm: i32 },
    Sltiu { rd: Reg, rs1: Reg, imm: i32 },
    Xori { rd: Reg, rs1: Reg, imm: i32 },
    Ori { rd: Reg, rs1: Reg, imm: i32 },
    Andi { rd: Reg, rs1: Reg, imm: i32 },
    Slli { rd: Reg, rs1: Reg, shamt: u8 },
    Srli { rd: Reg, rs1: Reg, shamt: u8 },
    Srai { rd: Reg, rs1: Reg, shamt: u8 },

    Add { rd: Reg, rs1: Reg, rs2: Reg },
    Sub { rd: Reg, rs1: Reg, rs2: Reg },
    Sll { rd: Reg, rs1: Reg, rs2: Reg },
    Slt { rd: Reg, rs1: Reg, rs2: Reg },
    Sltu { rd: Reg, rs1: Reg, rs2: Reg },
    Xor { rd: Reg, rs1: Reg, rs2: Reg },
    Srl { rd: Reg, rs1: Reg, rs2: Reg },
    Sra { rd: Reg, rs1: Reg, rs2: Reg },
    Or { rd: Reg, rs1: Reg, rs2: Reg },
    And { rd: Reg, rs1: Reg, rs2: Reg },

    // RV32M
    Mul { rd: Reg, rs1: Reg, rs2: Reg },
    Mulh { rd: Reg, rs1: Reg, rs2: Reg },
    Mulhsu { rd: Reg, rs1: Reg, rs2: Reg },
    Mulhu { rd: Reg, rs1: Reg, rs2: Reg },
    Div { rd: Reg, rs1: Reg, rs2: Reg },
    Divu { rd: Reg, rs1: Reg, rs2: Reg },
    Rem { rd: Reg, rs1: Reg, rs2: Reg },
    Remu { rd: Reg, rs1: Reg, rs2: Reg },

    Fence,
    Ecall,
    Ebreak,
}

#[inline]
fn sign_extend(val: u32, bits: u32) -> i32 {
    let shift = 32 - bits;
    ((val << shift) as i32) >> shift
}

#[inline]
fn imm_i(word: u32) -> i32 {
    (word as i32) >> 20
}

#[inline]
fn imm_s(word: u32) -> i32 {
    let raw = (((word >> 25) & 0x7F) << 5) | ((word >> 7) & 0x1F);
    sign_extend(raw, 12)
}

#[inline]
fn imm_b(word: u32) -> i32 {
    let b12 = (word >> 31) & 1;
    let b11 = (word >> 7) & 1;
    let b10_5 = (word >> 25) & 0x3F;
    let b4_1 = (word >> 8) & 0x0F;
    let raw = (b12 << 12) | (b11 << 11) | (b10_5 << 5) | (b4_1 << 1);
    sign_extend(raw, 13)
}

#[inline]
fn imm_u(word: u32) -> i32 {
    (word & 0xFFFF_F000) as i32
}

#[inline]
fn imm_j(word: u32) -> i32 {
    let b20 = (word >> 31) & 1;
    let b19_12 = (word >> 12) & 0xFF;
    let b11 = (word >> 20) & 1;
    let b10_1 = (word >> 21) & 0x3FF;
    let raw = (b20 << 20) | (b19_12 << 12) | (b11 << 11) | (b10_1 << 1);
    sign_extend(raw, 21)
}

/// Decode a single 32-bit RISC-V RV32IM instruction at `pc`.
pub fn decode_instruction(pc: u32, word: u32) -> Result<Instruction, DecodeError> {
    if (word & 0x3) != 0x3 {
        return Err(DecodeError::CompressedUnsupported { pc, word });
    }

    let opcode = word & 0x7F;
    let rd = Reg(((word >> 7) & 0x1F) as u8);
    let funct3 = (word >> 12) & 0x7;
    let rs1 = Reg(((word >> 15) & 0x1F) as u8);
    let rs2 = Reg(((word >> 20) & 0x1F) as u8);
    let funct7 = (word >> 25) & 0x7F;

    match opcode {
        0x37 => Ok(Instruction::Lui {
            rd,
            imm: imm_u(word),
        }),
        0x17 => Ok(Instruction::Auipc {
            rd,
            imm: imm_u(word),
        }),
        0x6F => Ok(Instruction::Jal {
            rd,
            imm: imm_j(word),
        }),
        0x67 if funct3 == 0 => Ok(Instruction::Jalr {
            rd,
            rs1,
            imm: imm_i(word),
        }),
        0x63 => {
            let imm = imm_b(word);
            match funct3 {
                0 => Ok(Instruction::Beq { rs1, rs2, imm }),
                1 => Ok(Instruction::Bne { rs1, rs2, imm }),
                4 => Ok(Instruction::Blt { rs1, rs2, imm }),
                5 => Ok(Instruction::Bge { rs1, rs2, imm }),
                6 => Ok(Instruction::Bltu { rs1, rs2, imm }),
                7 => Ok(Instruction::Bgeu { rs1, rs2, imm }),
                _ => Err(DecodeError::UnsupportedInstruction { pc, word }),
            }
        }
        0x03 => {
            let imm = imm_i(word);
            match funct3 {
                0 => Ok(Instruction::Lb { rd, rs1, imm }),
                1 => Ok(Instruction::Lh { rd, rs1, imm }),
                2 => Ok(Instruction::Lw { rd, rs1, imm }),
                4 => Ok(Instruction::Lbu { rd, rs1, imm }),
                5 => Ok(Instruction::Lhu { rd, rs1, imm }),
                _ => Err(DecodeError::UnsupportedInstruction { pc, word }),
            }
        }
        0x23 => {
            let imm = imm_s(word);
            match funct3 {
                0 => Ok(Instruction::Sb { rs1, rs2, imm }),
                1 => Ok(Instruction::Sh { rs1, rs2, imm }),
                2 => Ok(Instruction::Sw { rs1, rs2, imm }),
                _ => Err(DecodeError::UnsupportedInstruction { pc, word }),
            }
        }
        0x13 => {
            let imm = imm_i(word);
            let shamt = ((word >> 20) & 0x1F) as u8;
            match funct3 {
                0 => Ok(Instruction::Addi { rd, rs1, imm }),
                2 => Ok(Instruction::Slti { rd, rs1, imm }),
                3 => Ok(Instruction::Sltiu { rd, rs1, imm }),
                4 => Ok(Instruction::Xori { rd, rs1, imm }),
                6 => Ok(Instruction::Ori { rd, rs1, imm }),
                7 => Ok(Instruction::Andi { rd, rs1, imm }),
                1 if funct7 == 0x00 => Ok(Instruction::Slli { rd, rs1, shamt }),
                5 if funct7 == 0x00 => Ok(Instruction::Srli { rd, rs1, shamt }),
                5 if funct7 == 0x20 => Ok(Instruction::Srai { rd, rs1, shamt }),
                _ => Err(DecodeError::UnsupportedInstruction { pc, word }),
            }
        }
        0x33 => match (funct7, funct3) {
            (0x00, 0) => Ok(Instruction::Add { rd, rs1, rs2 }),
            (0x20, 0) => Ok(Instruction::Sub { rd, rs1, rs2 }),
            (0x00, 1) => Ok(Instruction::Sll { rd, rs1, rs2 }),
            (0x00, 2) => Ok(Instruction::Slt { rd, rs1, rs2 }),
            (0x00, 3) => Ok(Instruction::Sltu { rd, rs1, rs2 }),
            (0x00, 4) => Ok(Instruction::Xor { rd, rs1, rs2 }),
            (0x00, 5) => Ok(Instruction::Srl { rd, rs1, rs2 }),
            (0x20, 5) => Ok(Instruction::Sra { rd, rs1, rs2 }),
            (0x00, 6) => Ok(Instruction::Or { rd, rs1, rs2 }),
            (0x00, 7) => Ok(Instruction::And { rd, rs1, rs2 }),
            // RV32M
            (0x01, 0) => Ok(Instruction::Mul { rd, rs1, rs2 }),
            (0x01, 1) => Ok(Instruction::Mulh { rd, rs1, rs2 }),
            (0x01, 2) => Ok(Instruction::Mulhsu { rd, rs1, rs2 }),
            (0x01, 3) => Ok(Instruction::Mulhu { rd, rs1, rs2 }),
            (0x01, 4) => Ok(Instruction::Div { rd, rs1, rs2 }),
            (0x01, 5) => Ok(Instruction::Divu { rd, rs1, rs2 }),
            (0x01, 6) => Ok(Instruction::Rem { rd, rs1, rs2 }),
            (0x01, 7) => Ok(Instruction::Remu { rd, rs1, rs2 }),
            _ => Err(DecodeError::UnsupportedInstruction { pc, word }),
        },
        0x0F if funct3 == 0 || funct3 == 1 => Ok(Instruction::Fence),
        0x73 => match word {
            0x0000_0073 => Ok(Instruction::Ecall),
            0x0010_0073 => Ok(Instruction::Ebreak),
            _ => Err(DecodeError::UnsupportedInstruction { pc, word }),
        },
        _ => Err(DecodeError::UnsupportedInstruction { pc, word }),
    }
}
