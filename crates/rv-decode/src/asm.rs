use crate::inst::{FReg, Instruction, Reg};

#[inline]
fn encode_r4(opcode: u32, rd: FReg, funct3: u32, rs1: FReg, rs2: FReg, rs3: FReg) -> u32 {
    ((rs3.0 as u32 & 0x1F) << 27)
        | ((rs2.0 as u32 & 0x1F) << 20)
        | ((rs1.0 as u32 & 0x1F) << 15)
        | ((funct3 & 0x7) << 12)
        | ((rd.0 as u32 & 0x1F) << 7)
        | opcode
}

#[inline]
fn encode_r(opcode: u32, rd: Reg, funct3: u32, rs1: Reg, rs2: Reg, funct7: u32) -> u32 {
    (funct7 << 25)
        | ((rs2.0 as u32 & 0x1F) << 20)
        | ((rs1.0 as u32 & 0x1F) << 15)
        | (funct3 << 12)
        | ((rd.0 as u32 & 0x1F) << 7)
        | opcode
}

#[inline]
fn encode_i(opcode: u32, rd: Reg, funct3: u32, rs1: Reg, imm: i32) -> u32 {
    (((imm as u32) & 0xFFF) << 20)
        | ((rs1.0 as u32 & 0x1F) << 15)
        | (funct3 << 12)
        | ((rd.0 as u32 & 0x1F) << 7)
        | opcode
}

#[inline]
fn encode_s(opcode: u32, funct3: u32, rs1: Reg, rs2: Reg, imm: i32) -> u32 {
    let u = imm as u32;
    (((u >> 5) & 0x7F) << 25)
        | ((rs2.0 as u32 & 0x1F) << 20)
        | ((rs1.0 as u32 & 0x1F) << 15)
        | (funct3 << 12)
        | ((u & 0x1F) << 7)
        | opcode
}

#[inline]
fn encode_b(opcode: u32, funct3: u32, rs1: Reg, rs2: Reg, imm: i32) -> u32 {
    let u = imm as u32;
    let b12 = (u >> 12) & 1;
    let b11 = (u >> 11) & 1;
    let b10_5 = (u >> 5) & 0x3F;
    let b4_1 = (u >> 1) & 0x0F;
    (b12 << 31)
        | (b10_5 << 25)
        | ((rs2.0 as u32 & 0x1F) << 20)
        | ((rs1.0 as u32 & 0x1F) << 15)
        | (funct3 << 12)
        | (b4_1 << 8)
        | (b11 << 7)
        | opcode
}

#[inline]
fn encode_u(opcode: u32, rd: Reg, imm: i32) -> u32 {
    ((imm as u32) & 0xFFFF_F000) | ((rd.0 as u32 & 0x1F) << 7) | opcode
}

#[inline]
fn encode_j(opcode: u32, rd: Reg, imm: i32) -> u32 {
    let u = imm as u32;
    let b20 = (u >> 20) & 1;
    let b19_12 = (u >> 12) & 0xFF;
    let b11 = (u >> 11) & 1;
    let b10_1 = (u >> 1) & 0x3FF;
    (b20 << 31)
        | (b10_1 << 21)
        | (b11 << 20)
        | (b19_12 << 12)
        | ((rd.0 as u32 & 0x1F) << 7)
        | opcode
}

/// Encode an `Instruction` back into its 32-bit RISC-V machine word.
pub fn encode_instruction(inst: &Instruction) -> u32 {
    match *inst {
        Instruction::Lui { rd, imm } => encode_u(0x37, rd, imm),
        Instruction::Auipc { rd, imm } => encode_u(0x17, rd, imm),
        Instruction::Jal { rd, imm } => encode_j(0x6F, rd, imm),
        Instruction::Jalr { rd, rs1, imm } => encode_i(0x67, rd, 0, rs1, imm),

        Instruction::Beq { rs1, rs2, imm } => encode_b(0x63, 0, rs1, rs2, imm),
        Instruction::Bne { rs1, rs2, imm } => encode_b(0x63, 1, rs1, rs2, imm),
        Instruction::Blt { rs1, rs2, imm } => encode_b(0x63, 4, rs1, rs2, imm),
        Instruction::Bge { rs1, rs2, imm } => encode_b(0x63, 5, rs1, rs2, imm),
        Instruction::Bltu { rs1, rs2, imm } => encode_b(0x63, 6, rs1, rs2, imm),
        Instruction::Bgeu { rs1, rs2, imm } => encode_b(0x63, 7, rs1, rs2, imm),

        Instruction::Lb { rd, rs1, imm } => encode_i(0x03, rd, 0, rs1, imm),
        Instruction::Lh { rd, rs1, imm } => encode_i(0x03, rd, 1, rs1, imm),
        Instruction::Lw { rd, rs1, imm } => encode_i(0x03, rd, 2, rs1, imm),
        Instruction::Lbu { rd, rs1, imm } => encode_i(0x03, rd, 4, rs1, imm),
        Instruction::Lhu { rd, rs1, imm } => encode_i(0x03, rd, 5, rs1, imm),

        Instruction::Sb { rs1, rs2, imm } => encode_s(0x23, 0, rs1, rs2, imm),
        Instruction::Sh { rs1, rs2, imm } => encode_s(0x23, 1, rs1, rs2, imm),
        Instruction::Sw { rs1, rs2, imm } => encode_s(0x23, 2, rs1, rs2, imm),

        Instruction::Addi { rd, rs1, imm } => encode_i(0x13, rd, 0, rs1, imm),
        Instruction::Slti { rd, rs1, imm } => encode_i(0x13, rd, 2, rs1, imm),
        Instruction::Sltiu { rd, rs1, imm } => encode_i(0x13, rd, 3, rs1, imm),
        Instruction::Xori { rd, rs1, imm } => encode_i(0x13, rd, 4, rs1, imm),
        Instruction::Ori { rd, rs1, imm } => encode_i(0x13, rd, 6, rs1, imm),
        Instruction::Andi { rd, rs1, imm } => encode_i(0x13, rd, 7, rs1, imm),
        Instruction::Slli { rd, rs1, shamt } => encode_r(0x13, rd, 1, rs1, Reg(shamt & 0x1F), 0x00),
        Instruction::Srli { rd, rs1, shamt } => encode_r(0x13, rd, 5, rs1, Reg(shamt & 0x1F), 0x00),
        Instruction::Srai { rd, rs1, shamt } => encode_r(0x13, rd, 5, rs1, Reg(shamt & 0x1F), 0x20),

        Instruction::Add { rd, rs1, rs2 } => encode_r(0x33, rd, 0, rs1, rs2, 0x00),
        Instruction::Sub { rd, rs1, rs2 } => encode_r(0x33, rd, 0, rs1, rs2, 0x20),
        Instruction::Sll { rd, rs1, rs2 } => encode_r(0x33, rd, 1, rs1, rs2, 0x00),
        Instruction::Slt { rd, rs1, rs2 } => encode_r(0x33, rd, 2, rs1, rs2, 0x00),
        Instruction::Sltu { rd, rs1, rs2 } => encode_r(0x33, rd, 3, rs1, rs2, 0x00),
        Instruction::Xor { rd, rs1, rs2 } => encode_r(0x33, rd, 4, rs1, rs2, 0x00),
        Instruction::Srl { rd, rs1, rs2 } => encode_r(0x33, rd, 5, rs1, rs2, 0x00),
        Instruction::Sra { rd, rs1, rs2 } => encode_r(0x33, rd, 5, rs1, rs2, 0x20),
        Instruction::Or { rd, rs1, rs2 } => encode_r(0x33, rd, 6, rs1, rs2, 0x00),
        Instruction::And { rd, rs1, rs2 } => encode_r(0x33, rd, 7, rs1, rs2, 0x00),

        Instruction::Mul { rd, rs1, rs2 } => encode_r(0x33, rd, 0, rs1, rs2, 0x01),
        Instruction::Mulh { rd, rs1, rs2 } => encode_r(0x33, rd, 1, rs1, rs2, 0x01),
        Instruction::Mulhsu { rd, rs1, rs2 } => encode_r(0x33, rd, 2, rs1, rs2, 0x01),
        Instruction::Mulhu { rd, rs1, rs2 } => encode_r(0x33, rd, 3, rs1, rs2, 0x01),
        Instruction::Div { rd, rs1, rs2 } => encode_r(0x33, rd, 4, rs1, rs2, 0x01),
        Instruction::Divu { rd, rs1, rs2 } => encode_r(0x33, rd, 5, rs1, rs2, 0x01),
        Instruction::Rem { rd, rs1, rs2 } => encode_r(0x33, rd, 6, rs1, rs2, 0x01),
        Instruction::Remu { rd, rs1, rs2 } => encode_r(0x33, rd, 7, rs1, rs2, 0x01),

        Instruction::Flw { rd, rs1, imm } => encode_i(0x07, Reg(rd.0), 2, rs1, imm),
        Instruction::Fsw { rs1, rs2, imm } => encode_s(0x27, 2, rs1, Reg(rs2.0), imm),
        Instruction::FmaddS { rd, rs1, rs2, rs3, rm } => encode_r4(0x43, rd, rm as u32, rs1, rs2, rs3),
        Instruction::FmsubS { rd, rs1, rs2, rs3, rm } => encode_r4(0x47, rd, rm as u32, rs1, rs2, rs3),
        Instruction::FnmsubS { rd, rs1, rs2, rs3, rm } => encode_r4(0x4B, rd, rm as u32, rs1, rs2, rs3),
        Instruction::FnmaddS { rd, rs1, rs2, rs3, rm } => encode_r4(0x4F, rd, rm as u32, rs1, rs2, rs3),
        Instruction::FaddS { rd, rs1, rs2, rm } => encode_r(0x53, Reg(rd.0), rm as u32, Reg(rs1.0), Reg(rs2.0), 0x00),
        Instruction::FsubS { rd, rs1, rs2, rm } => encode_r(0x53, Reg(rd.0), rm as u32, Reg(rs1.0), Reg(rs2.0), 0x04),
        Instruction::FmulS { rd, rs1, rs2, rm } => encode_r(0x53, Reg(rd.0), rm as u32, Reg(rs1.0), Reg(rs2.0), 0x08),
        Instruction::FdivS { rd, rs1, rs2, rm } => encode_r(0x53, Reg(rd.0), rm as u32, Reg(rs1.0), Reg(rs2.0), 0x0C),
        Instruction::FsqrtS { rd, rs1, rm } => encode_r(0x53, Reg(rd.0), rm as u32, Reg(rs1.0), Reg(0), 0x2C),
        Instruction::FsgnjS { rd, rs1, rs2 } => encode_r(0x53, Reg(rd.0), 0, Reg(rs1.0), Reg(rs2.0), 0x10),
        Instruction::FsgnjnS { rd, rs1, rs2 } => encode_r(0x53, Reg(rd.0), 1, Reg(rs1.0), Reg(rs2.0), 0x10),
        Instruction::FsgnjxS { rd, rs1, rs2 } => encode_r(0x53, Reg(rd.0), 2, Reg(rs1.0), Reg(rs2.0), 0x10),
        Instruction::FminS { rd, rs1, rs2 } => encode_r(0x53, Reg(rd.0), 0, Reg(rs1.0), Reg(rs2.0), 0x14),
        Instruction::FmaxS { rd, rs1, rs2 } => encode_r(0x53, Reg(rd.0), 1, Reg(rs1.0), Reg(rs2.0), 0x14),
        Instruction::FcvtWS { rd, rs1, rm } => encode_r(0x53, rd, rm as u32, Reg(rs1.0), Reg(0), 0x60),
        Instruction::FcvtWuS { rd, rs1, rm } => encode_r(0x53, rd, rm as u32, Reg(rs1.0), Reg(1), 0x60),
        Instruction::FmvXW { rd, rs1 } => encode_r(0x53, rd, 0, Reg(rs1.0), Reg(0), 0x70),
        Instruction::FclassS { rd, rs1 } => encode_r(0x53, rd, 1, Reg(rs1.0), Reg(0), 0x70),
        Instruction::FeqS { rd, rs1, rs2 } => encode_r(0x53, rd, 2, Reg(rs1.0), Reg(rs2.0), 0x50),
        Instruction::FltS { rd, rs1, rs2 } => encode_r(0x53, rd, 1, Reg(rs1.0), Reg(rs2.0), 0x50),
        Instruction::FleS { rd, rs1, rs2 } => encode_r(0x53, rd, 0, Reg(rs1.0), Reg(rs2.0), 0x50),
        Instruction::FcvtSW { rd, rs1, rm } => encode_r(0x53, Reg(rd.0), rm as u32, rs1, Reg(0), 0x68),
        Instruction::FcvtSWu { rd, rs1, rm } => encode_r(0x53, Reg(rd.0), rm as u32, rs1, Reg(1), 0x68),
        Instruction::FmvWX { rd, rs1 } => encode_r(0x53, Reg(rd.0), 0, rs1, Reg(0), 0x78),

        Instruction::Csrrw { rd, rs1, csr } => encode_i(0x73, rd, 1, rs1, csr as i32),
        Instruction::Csrrs { rd, rs1, csr } => encode_i(0x73, rd, 2, rs1, csr as i32),
        Instruction::Csrrc { rd, rs1, csr } => encode_i(0x73, rd, 3, rs1, csr as i32),
        Instruction::Csrrwi { rd, zimm, csr } => encode_i(0x73, rd, 5, Reg(zimm & 0x1F), csr as i32),
        Instruction::Csrrsi { rd, zimm, csr } => encode_i(0x73, rd, 6, Reg(zimm & 0x1F), csr as i32),
        Instruction::Csrrci { rd, zimm, csr } => encode_i(0x73, rd, 7, Reg(zimm & 0x1F), csr as i32),

        Instruction::Fence => 0x0000_000F,
        Instruction::Ecall => 0x0000_0073,
        Instruction::Ebreak => 0x0010_0073,
    }
}

/// Build a minimal static little-endian ELF32 RISC-V executable from raw machine words
/// (plus optional read-only data segment). Used by compliance and differential tests.
pub fn build_test_elf(words: &[u32], rodata: &[u8]) -> Vec<u8> {
    build_test_elf_at(rv_abi::TEXT_BASE, words, rodata)
}

pub fn build_test_elf_at(entry: u32, words: &[u32], rodata: &[u8]) -> Vec<u8> {
    let ehdr_size: u32 = 52;
    let phdr_size: u32 = 32;
    let num_phdrs: u32 = if rodata.is_empty() { 1 } else { 2 };
    let text_offset: u32 = ehdr_size + phdr_size * num_phdrs;
    let text_size: u32 = (words.len() * 4) as u32;
    let rodata_offset: u32 = text_offset + text_size;
    let rodata_vaddr: u32 = entry + text_size + 0x100;
    let rodata_size: u32 = rodata.len() as u32;

    let mut elf = Vec::new();
    // ELF32 Header (52 bytes)
    elf.extend_from_slice(b"\x7fELF");
    elf.push(1); // ELFCLASS32
    elf.push(1); // ELFDATA2LSB
    elf.push(1); // EV_CURRENT
    elf.push(0); // ELFOSABI_NONE
    elf.extend_from_slice(&[0u8; 8]); // padding
    elf.extend_from_slice(&2u16.to_le_bytes()); // ET_EXEC
    elf.extend_from_slice(&243u16.to_le_bytes()); // EM_RISCV
    elf.extend_from_slice(&1u32.to_le_bytes()); // EV_CURRENT
    elf.extend_from_slice(&entry.to_le_bytes()); // e_entry
    elf.extend_from_slice(&ehdr_size.to_le_bytes()); // e_phoff
    elf.extend_from_slice(&0u32.to_le_bytes()); // e_shoff
    elf.extend_from_slice(&0u32.to_le_bytes()); // e_flags
    elf.extend_from_slice(&(ehdr_size as u16).to_le_bytes()); // e_ehsize
    elf.extend_from_slice(&(phdr_size as u16).to_le_bytes()); // e_phentsize
    elf.extend_from_slice(&(num_phdrs as u16).to_le_bytes()); // e_phnum
    elf.extend_from_slice(&40u16.to_le_bytes()); // e_shentsize
    elf.extend_from_slice(&0u16.to_le_bytes()); // e_shnum
    elf.extend_from_slice(&0u16.to_le_bytes()); // e_shstrndx

    // PT_LOAD #1: .text (PF_R | PF_X = 5)
    elf.extend_from_slice(&1u32.to_le_bytes()); // p_type = PT_LOAD
    elf.extend_from_slice(&text_offset.to_le_bytes()); // p_offset
    elf.extend_from_slice(&entry.to_le_bytes()); // p_vaddr
    elf.extend_from_slice(&entry.to_le_bytes()); // p_paddr
    elf.extend_from_slice(&text_size.to_le_bytes()); // p_filesz
    elf.extend_from_slice(&text_size.to_le_bytes()); // p_memsz
    elf.extend_from_slice(&5u32.to_le_bytes()); // p_flags = PF_R | PF_X
    elf.extend_from_slice(&4u32.to_le_bytes()); // p_align

    if !rodata.is_empty() {
        // PT_LOAD #2: .rodata/.data (PF_R | PF_W = 6)
        elf.extend_from_slice(&1u32.to_le_bytes());
        elf.extend_from_slice(&rodata_offset.to_le_bytes());
        elf.extend_from_slice(&rodata_vaddr.to_le_bytes());
        elf.extend_from_slice(&rodata_vaddr.to_le_bytes());
        elf.extend_from_slice(&rodata_size.to_le_bytes());
        elf.extend_from_slice(&rodata_size.to_le_bytes());
        elf.extend_from_slice(&6u32.to_le_bytes());
        elf.extend_from_slice(&4u32.to_le_bytes());
    }

    for w in words {
        elf.extend_from_slice(&w.to_le_bytes());
    }
    elf.extend_from_slice(rodata);
    elf
}
