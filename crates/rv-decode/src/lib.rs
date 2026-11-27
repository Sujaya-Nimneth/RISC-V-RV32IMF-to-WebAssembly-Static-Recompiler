//! RV32IMF instruction decoder, disassembler, ELF32 loader, and CFG discovery.

pub mod asm;
pub mod cfg;
pub mod disasm;
pub mod elf;
pub mod inst;

pub use asm::{build_test_elf, build_test_elf_at, encode_instruction};
pub use cfg::{BasicBlock, BlockTerm, CfgError, DecodedInst, FunctionCfg, ProgramCfg, discover_cfg};
pub use disasm::disassemble;
pub use elf::{ElfError, ElfSymbol, LoadSegment, LoadedElf};
pub use inst::{DecodeError, FReg, Instruction, Reg, decode_instruction};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_rv32im_instructions() {
        let samples = [
            Instruction::Lui {
                rd: Reg(5),
                imm: 0x1234_5000,
            },
            Instruction::Auipc {
                rd: Reg(6),
                imm: -0x1000,
            },
            Instruction::Jal {
                rd: Reg::RA,
                imm: -16,
            },
            Instruction::Jalr {
                rd: Reg::ZERO,
                rs1: Reg::RA,
                imm: 0,
            },
            Instruction::Beq {
                rs1: Reg::A0,
                rs2: Reg::A1,
                imm: 24,
            },
            Instruction::Bgeu {
                rs1: Reg::A0,
                rs2: Reg::A1,
                imm: -12,
            },
            Instruction::Lw {
                rd: Reg::A0,
                rs1: Reg::SP,
                imm: -16,
            },
            Instruction::Sw {
                rs1: Reg::SP,
                rs2: Reg::RA,
                imm: 12,
            },
            Instruction::Addi {
                rd: Reg::A0,
                rs1: Reg::ZERO,
                imm: -42,
            },
            Instruction::Srai {
                rd: Reg::A0,
                rs1: Reg::A1,
                shamt: 31,
            },
            Instruction::Sub {
                rd: Reg::A0,
                rs1: Reg::A1,
                rs2: Reg::A2,
            },
            Instruction::Mulhsu {
                rd: Reg::A0,
                rs1: Reg::A1,
                rs2: Reg::A2,
            },
            Instruction::Div {
                rd: Reg::A0,
                rs1: Reg::A1,
                rs2: Reg::A2,
            },
            Instruction::Remu {
                rd: Reg::A0,
                rs1: Reg::A1,
                rs2: Reg::A2,
            },
            Instruction::Fence,
            Instruction::Ecall,
            Instruction::Ebreak,
        ];
        for inst in samples {
            let w = encode_instruction(&inst);
            let decoded = decode_instruction(0x0001_0000, w).expect("decode");
            assert_eq!(decoded, inst);
            assert!(!disassemble(0x0001_0000, &decoded).is_empty());
        }
    }

    #[test]
    fn roundtrip_rv32f_and_csr_instructions() {
        let samples = [
            Instruction::Flw { rd: FReg(1), rs1: Reg::SP, imm: -16 },
            Instruction::Fsw { rs1: Reg::SP, rs2: FReg(2), imm: 12 },
            Instruction::FmaddS { rd: FReg(3), rs1: FReg(1), rs2: FReg(2), rs3: FReg(4), rm: 7 },
            Instruction::FmsubS { rd: FReg(3), rs1: FReg(1), rs2: FReg(2), rs3: FReg(4), rm: 0 },
            Instruction::FnmsubS { rd: FReg(3), rs1: FReg(1), rs2: FReg(2), rs3: FReg(4), rm: 7 },
            Instruction::FnmaddS { rd: FReg(3), rs1: FReg(1), rs2: FReg(2), rs3: FReg(4), rm: 7 },
            Instruction::FaddS { rd: FReg(5), rs1: FReg(1), rs2: FReg(2), rm: 7 },
            Instruction::FsubS { rd: FReg(5), rs1: FReg(1), rs2: FReg(2), rm: 7 },
            Instruction::FmulS { rd: FReg(5), rs1: FReg(1), rs2: FReg(2), rm: 7 },
            Instruction::FdivS { rd: FReg(5), rs1: FReg(1), rs2: FReg(2), rm: 7 },
            Instruction::FsqrtS { rd: FReg(5), rs1: FReg(1), rm: 7 },
            Instruction::FsgnjS { rd: FReg(5), rs1: FReg(1), rs2: FReg(2) },
            Instruction::FsgnjnS { rd: FReg(5), rs1: FReg(1), rs2: FReg(2) },
            Instruction::FsgnjxS { rd: FReg(5), rs1: FReg(1), rs2: FReg(2) },
            Instruction::FminS { rd: FReg(5), rs1: FReg(1), rs2: FReg(2) },
            Instruction::FmaxS { rd: FReg(5), rs1: FReg(1), rs2: FReg(2) },
            Instruction::FcvtWS { rd: Reg::A0, rs1: FReg(1), rm: 7 },
            Instruction::FcvtWuS { rd: Reg::A0, rs1: FReg(1), rm: 7 },
            Instruction::FmvXW { rd: Reg::A0, rs1: FReg(1) },
            Instruction::FclassS { rd: Reg::A0, rs1: FReg(1) },
            Instruction::FeqS { rd: Reg::A0, rs1: FReg(1), rs2: FReg(2) },
            Instruction::FltS { rd: Reg::A0, rs1: FReg(1), rs2: FReg(2) },
            Instruction::FleS { rd: Reg::A0, rs1: FReg(1), rs2: FReg(2) },
            Instruction::FcvtSW { rd: FReg(1), rs1: Reg::A0, rm: 7 },
            Instruction::FcvtSWu { rd: FReg(1), rs1: Reg::A0, rm: 7 },
            Instruction::FmvWX { rd: FReg(1), rs1: Reg::A0 },
            Instruction::Csrrw { rd: Reg::A0, rs1: Reg::A1, csr: 0x003 },
            Instruction::Csrrsi { rd: Reg::A0, zimm: 5, csr: 0x002 },
        ];
        for inst in samples {
            let w = encode_instruction(&inst);
            let decoded = decode_instruction(0x0001_0000, w).expect("decode");
            assert_eq!(decoded, inst);
            assert!(!disassemble(0x0001_0000, &decoded).is_empty());
        }
    }

    #[test]
    fn rejects_compressed_instructions() {
        let err = decode_instruction(0x10000, 0x0000_0001).unwrap_err();
        assert!(matches!(err, DecodeError::CompressedUnsupported { .. }));
    }

    #[test]
    fn parses_hello_smoke_elf() {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../guests/common/hello.elf");
        let bytes = std::fs::read(path).expect("read hello.elf");
        let elf = LoadedElf::parse(&bytes).expect("parse hello.elf");
        assert_eq!(elf.entry, rv_abi::TEXT_BASE);
        assert!(elf.symbol_name(elf.entry).is_some());
    }
}
