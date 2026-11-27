use proptest::prelude::*;
use rv_abi::{HeadlessHost, SYS_EXIT};
use rv_decode::{Instruction, LoadedElf, Reg, build_test_elf, discover_cfg, encode_instruction};
use rv_interp::Interpreter;
use rv_ir::{OptConfig, lift_program};
use rv_wasm::{compile_to_wasm_with_opts, runtime::execute_wasm};

#[derive(Debug, Clone)]
enum FuzzOp {
    AluImm(u8, u8, u8, i16),
    AluRR(u8, u8, u8, u8),
    MulDiv(u8, u8, u8, u8),
    MemRoundTrip(u8, u8, i16),
    CondBranch(u8, u8, u8, bool),
}

fn arb_reg() -> impl Strategy<Value = u8> {
    // Use t0..t2 (5..7), a0..a5 (10..15), s0..s1 (8..9) — avoid sp (2) and a7 (17)
    prop_oneof![
        Just(0u8),
        5u8..=15u8,
    ]
}

fn arb_fuzz_op() -> impl Strategy<Value = FuzzOp> {
    prop_oneof![
        (0u8..9, arb_reg(), arb_reg(), -2048i16..2048).prop_map(|(k, d, s, i)| FuzzOp::AluImm(k, d, s, i)),
        (0u8..10, arb_reg(), arb_reg(), arb_reg()).prop_map(|(k, d, s1, s2)| FuzzOp::AluRR(k, d, s1, s2)),
        (0u8..8, arb_reg(), arb_reg(), arb_reg()).prop_map(|(k, d, s1, s2)| FuzzOp::MulDiv(k, d, s1, s2)),
        (0u8..5, arb_reg(), -128i16..0).prop_map(|(k, d, off)| FuzzOp::MemRoundTrip(k, d, off & !3)),
        (0u8..6, arb_reg(), arb_reg(), any::<bool>()).prop_map(|(k, s1, s2, skip)| FuzzOp::CondBranch(k, s1, s2, skip)),
    ]
}

fn compile_fuzz_program(ops: &[FuzzOp]) -> Vec<u8> {
    let mut words = Vec::new();
    // Seed registers with interesting edge values (0, -1, INT_MIN, 42)
    words.push(encode_instruction(&Instruction::Addi { rd: Reg(5), rs1: Reg::ZERO, imm: 0 }));
    words.push(encode_instruction(&Instruction::Addi { rd: Reg(6), rs1: Reg::ZERO, imm: -1 }));
    words.push(encode_instruction(&Instruction::Lui { rd: Reg(7), imm: i32::MIN }));
    words.push(encode_instruction(&Instruction::Addi { rd: Reg(8), rs1: Reg::ZERO, imm: 42 }));

    for op in ops {
        match *op {
            FuzzOp::AluImm(k, d, s, imm) => {
                let rd = Reg(d);
                let rs1 = Reg(s);
                let shamt = (imm as u8) & 0x1F;
                let inst = match k {
                    0 => Instruction::Addi { rd, rs1, imm: imm as i32 },
                    1 => Instruction::Slti { rd, rs1, imm: imm as i32 },
                    2 => Instruction::Sltiu { rd, rs1, imm: imm as i32 },
                    3 => Instruction::Xori { rd, rs1, imm: imm as i32 },
                    4 => Instruction::Ori { rd, rs1, imm: imm as i32 },
                    5 => Instruction::Andi { rd, rs1, imm: imm as i32 },
                    6 => Instruction::Slli { rd, rs1, shamt },
                    7 => Instruction::Srli { rd, rs1, shamt },
                    _ => Instruction::Srai { rd, rs1, shamt },
                };
                words.push(encode_instruction(&inst));
            }
            FuzzOp::AluRR(k, d, s1, s2) => {
                let (rd, rs1, rs2) = (Reg(d), Reg(s1), Reg(s2));
                let inst = match k {
                    0 => Instruction::Add { rd, rs1, rs2 },
                    1 => Instruction::Sub { rd, rs1, rs2 },
                    2 => Instruction::Sll { rd, rs1, rs2 },
                    3 => Instruction::Slt { rd, rs1, rs2 },
                    4 => Instruction::Sltu { rd, rs1, rs2 },
                    5 => Instruction::Xor { rd, rs1, rs2 },
                    6 => Instruction::Srl { rd, rs1, rs2 },
                    7 => Instruction::Sra { rd, rs1, rs2 },
                    8 => Instruction::Or { rd, rs1, rs2 },
                    _ => Instruction::And { rd, rs1, rs2 },
                };
                words.push(encode_instruction(&inst));
            }
            FuzzOp::MulDiv(k, d, s1, s2) => {
                let (rd, rs1, rs2) = (Reg(d), Reg(s1), Reg(s2));
                let inst = match k {
                    0 => Instruction::Mul { rd, rs1, rs2 },
                    1 => Instruction::Mulh { rd, rs1, rs2 },
                    2 => Instruction::Mulhsu { rd, rs1, rs2 },
                    3 => Instruction::Mulhu { rd, rs1, rs2 },
                    4 => Instruction::Div { rd, rs1, rs2 },
                    5 => Instruction::Divu { rd, rs1, rs2 },
                    6 => Instruction::Rem { rd, rs1, rs2 },
                    _ => Instruction::Remu { rd, rs1, rs2 },
                };
                words.push(encode_instruction(&inst));
            }
            FuzzOp::MemRoundTrip(k, d, off) => {
                // Store d to [sp + off] (negative offset from STACK_TOP), then load back
                let rd = Reg(d);
                words.push(encode_instruction(&Instruction::Sw { rs1: Reg::SP, rs2: rd, imm: off as i32 }));
                let load = match k {
                    0 => Instruction::Lb { rd, rs1: Reg::SP, imm: off as i32 },
                    1 => Instruction::Lh { rd, rs1: Reg::SP, imm: off as i32 },
                    2 => Instruction::Lw { rd, rs1: Reg::SP, imm: off as i32 },
                    3 => Instruction::Lbu { rd, rs1: Reg::SP, imm: off as i32 },
                    _ => Instruction::Lhu { rd, rs1: Reg::SP, imm: off as i32 },
                };
                words.push(encode_instruction(&load));
            }
            FuzzOp::CondBranch(k, s1, s2, _skip) => {
                let (rs1, rs2) = (Reg(s1), Reg(s2));
                let br = match k {
                    0 => Instruction::Beq { rs1, rs2, imm: 8 },
                    1 => Instruction::Bne { rs1, rs2, imm: 8 },
                    2 => Instruction::Blt { rs1, rs2, imm: 8 },
                    3 => Instruction::Bge { rs1, rs2, imm: 8 },
                    4 => Instruction::Bltu { rs1, rs2, imm: 8 },
                    _ => Instruction::Bgeu { rs1, rs2, imm: 8 },
                };
                words.push(encode_instruction(&br));
                words.push(encode_instruction(&Instruction::Addi { rd: Reg(9), rs1: Reg(9), imm: 1 }));
            }
        }
    }

    words.push(encode_instruction(&Instruction::Addi { rd: Reg::A0, rs1: Reg::ZERO, imm: 0 }));
    words.push(encode_instruction(&Instruction::Addi { rd: Reg::A7, rs1: Reg::ZERO, imm: SYS_EXIT as i32 }));
    words.push(encode_instruction(&Instruction::Ecall));
    build_test_elf(&words, &[])
}

fn check_parity(elf_bytes: &[u8], opt: &OptConfig) {
    let elf = LoadedElf::parse(elf_bytes).expect("parse");
    let cfg = discover_cfg(&elf).expect("cfg");
    let ir = lift_program(&cfg, opt);
    let compiled = compile_to_wasm_with_opts(&ir, &elf, opt.local_coalescing).expect("compile");
    assert!(compiled.valid);

    let mut interp = Interpreter::from_elf(&elf);
    let mut i_host = HeadlessHost::new();
    interp.run(&mut i_host, 100_000).expect("interp");

    let (w_res, _) = execute_wasm(&compiled.wasm_bytes, HeadlessHost::new()).expect("wasm");
    assert_eq!(i_host.exit_code, Some(w_res.exit_code));
    assert_eq!(interp.cpu.x, w_res.regs);
    assert_eq!(interp.memory_crc32(), w_res.memory_crc32);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(48))]

    #[test]
    fn fuzz_rv32im_o0_and_o2_parity(ops in prop::collection::vec(arb_fuzz_op(), 5..30)) {
        let elf_bytes = compile_fuzz_program(&ops);
        check_parity(&elf_bytes, &OptConfig::o0());
        check_parity(&elf_bytes, &OptConfig::o2());
    }
}
