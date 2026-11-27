//! RV32IMF instruction decoder, disassembler, ELF32 loader, and CFG discovery.

pub mod asm;
pub mod disasm;
pub mod elf;
pub mod inst;

pub use asm::{build_test_elf, build_test_elf_at, encode_instruction};
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
