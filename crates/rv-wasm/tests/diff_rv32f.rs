use proptest::prelude::*;
use rv_abi::{HeadlessHost, SYS_EXIT};
use rv_decode::{FReg, Instruction, LoadedElf, Reg, build_test_elf, discover_cfg, encode_instruction};
use rv_interp::Interpreter;
use rv_ir::{OptConfig, lift_program};
use rv_wasm::{compile_to_wasm_with_opts, runtime::execute_wasm};

fn assert_rv32f_parity(words: &[u32], opt: &OptConfig) {
    let elf_bytes = build_test_elf(words, &[]);
    let elf = LoadedElf::parse(&elf_bytes).expect("parse");
    let cfg = discover_cfg(&elf).expect("cfg");
    let ir = lift_program(&cfg, opt);
    let compiled = compile_to_wasm_with_opts(&ir, &elf, opt.local_coalescing).expect("compile");
    assert!(compiled.valid, "invalid wasm: {:?}", compiled.validation_error);

    let mut interp = Interpreter::from_elf(&elf);
    let mut i_host = HeadlessHost::new();
    interp.run(&mut i_host, 100_000).expect("interp");

    let (w_res, _) = execute_wasm(&compiled.wasm_bytes, HeadlessHost::new()).expect("wasm");
    assert_eq!(i_host.exit_code, Some(w_res.exit_code));
    assert_eq!(interp.cpu.x, w_res.regs);
    for fr in 0..32 {
        assert_eq!(
            interp.cpu.f[fr].to_bits(),
            w_res.fregs_bits[fr],
            "f{fr} bit mismatch: interp=0x{:08x}, wasm=0x{:08x}",
            interp.cpu.f[fr].to_bits(),
            w_res.fregs_bits[fr]
        );
    }
    assert_eq!(interp.memory_crc32(), w_res.memory_crc32);
}

fn load_f32_into(rd: FReg, tmp: Reg, bits: u32, out: &mut Vec<u32>) {
    let lo = ((bits as i32) << 20) >> 20;
    let hi = bits.wrapping_sub(lo as u32) as i32;
    out.push(encode_instruction(&Instruction::Lui { rd: tmp, imm: hi }));
    out.push(encode_instruction(&Instruction::Addi { rd: tmp, rs1: tmp, imm: lo }));
    out.push(encode_instruction(&Instruction::FmvWX { rd, rs1: tmp }));
}

fn exit_seq(code: i32) -> [u32; 3] {
    [
        encode_instruction(&Instruction::Addi { rd: Reg::A0, rs1: Reg::ZERO, imm: code }),
        encode_instruction(&Instruction::Addi { rd: Reg::A7, rs1: Reg::ZERO, imm: SYS_EXIT as i32 }),
        encode_instruction(&Instruction::Ecall),
    ]
}

#[test]
fn diff_rv32f_edge_cases_nan_fmin_fmax_fcvt_fclass_fma() {
    let mut w = Vec::new();
    // Load edge-case floats: -0.0, +0.0, +inf, -inf, qNaN (payload), sNaN, 1.5, 2.5, 1e30, 1e-30
    load_f32_into(FReg(1), Reg(5), 0x8000_0000, &mut w); // -0.0
    load_f32_into(FReg(2), Reg(5), 0x0000_0000, &mut w); // +0.0
    load_f32_into(FReg(3), Reg(5), 0x7FC0_1234, &mut w); // qNaN with payload
    load_f32_into(FReg(4), Reg(5), 0x7F80_0001, &mut w); // sNaN
    load_f32_into(FReg(5), Reg(5), 0x7F80_0000, &mut w); // +inf
    load_f32_into(FReg(6), Reg(5), 0xFF80_0000, &mut w); // -inf
    load_f32_into(FReg(7), Reg(5), 1.5f32.to_bits(), &mut w);
    load_f32_into(FReg(8), Reg(5), (-2.5f32).to_bits(), &mut w);
    load_f32_into(FReg(9), Reg(5), 0x0000_0001, &mut w); // +subnormal

    // FMIN / FMAX: -0.0 vs +0.0, NaN vs 1.5, NaN vs NaN
    w.push(encode_instruction(&Instruction::FminS { rd: FReg(10), rs1: FReg(1), rs2: FReg(2) }));
    w.push(encode_instruction(&Instruction::FmaxS { rd: FReg(11), rs1: FReg(1), rs2: FReg(2) }));
    w.push(encode_instruction(&Instruction::FminS { rd: FReg(12), rs1: FReg(3), rs2: FReg(7) }));
    w.push(encode_instruction(&Instruction::FmaxS { rd: FReg(13), rs1: FReg(7), rs2: FReg(3) }));
    w.push(encode_instruction(&Instruction::FminS { rd: FReg(14), rs1: FReg(3), rs2: FReg(4) }));

    // FCVT.W.S / FCVT.WU.S: NaN, +inf, -inf, -0.4, 2.5
    w.push(encode_instruction(&Instruction::FcvtWS { rd: Reg(6), rs1: FReg(3), rm: 7 }));
    w.push(encode_instruction(&Instruction::FcvtWuS { rd: Reg(7), rs1: FReg(3), rm: 7 }));
    w.push(encode_instruction(&Instruction::FcvtWS { rd: Reg(8), rs1: FReg(6), rm: 7 }));
    w.push(encode_instruction(&Instruction::FcvtWuS { rd: Reg(9), rs1: FReg(8), rm: 7 }));
    w.push(encode_instruction(&Instruction::FcvtWS { rd: Reg(11), rs1: FReg(7), rm: 7 }));

    // FCLASS.S
    w.push(encode_instruction(&Instruction::FclassS { rd: Reg(12), rs1: FReg(1) }));
    w.push(encode_instruction(&Instruction::FclassS { rd: Reg(13), rs1: FReg(4) }));
    w.push(encode_instruction(&Instruction::FclassS { rd: Reg(14), rs1: FReg(9) }));
    w.push(encode_instruction(&Instruction::FclassS { rd: Reg(15), rs1: FReg(5) }));

    // Single-rounding FMA test: (1.0 + 2^-23) * (1.0 + 2^-23) + (-1.0)
    load_f32_into(FReg(15), Reg(16), 0x3F80_0001, &mut w);
    load_f32_into(FReg(16), Reg(16), 0xBF80_0000, &mut w);
    w.push(encode_instruction(&Instruction::FmaddS { rd: FReg(17), rs1: FReg(15), rs2: FReg(15), rs3: FReg(16), rm: 7 }));
    w.push(encode_instruction(&Instruction::FmsubS { rd: FReg(18), rs1: FReg(7), rs2: FReg(8), rs3: FReg(7), rm: 7 }));
    w.push(encode_instruction(&Instruction::FnmsubS { rd: FReg(19), rs1: FReg(7), rs2: FReg(8), rs3: FReg(7), rm: 7 }));
    w.push(encode_instruction(&Instruction::FnmaddS { rd: FReg(20), rs1: FReg(7), rs2: FReg(8), rs3: FReg(7), rm: 7 }));

    // FLW / FSW roundtrip + FSGNJ* + FEQ/FLT/FLE
    w.push(encode_instruction(&Instruction::Fsw { rs1: Reg::SP, rs2: FReg(17), imm: -16 }));
    w.push(encode_instruction(&Instruction::Flw { rd: FReg(21), rs1: Reg::SP, imm: -16 }));
    w.push(encode_instruction(&Instruction::FsgnjS { rd: FReg(22), rs1: FReg(7), rs2: FReg(8) }));
    w.push(encode_instruction(&Instruction::FsgnjnS { rd: FReg(23), rs1: FReg(7), rs2: FReg(8) }));
    w.push(encode_instruction(&Instruction::FsgnjxS { rd: FReg(24), rs1: FReg(8), rs2: FReg(8) }));
    w.push(encode_instruction(&Instruction::FeqS { rd: Reg(18), rs1: FReg(1), rs2: FReg(2) }));
    w.push(encode_instruction(&Instruction::FltS { rd: Reg(19), rs1: FReg(8), rs2: FReg(7) }));
    w.push(encode_instruction(&Instruction::FleS { rd: Reg(20), rs1: FReg(3), rs2: FReg(7) }));

    w.extend_from_slice(&exit_seq(0));
    assert_rv32f_parity(&w, &OptConfig::o0());
    assert_rv32f_parity(&w, &OptConfig::o2());
}

const EDGE_FLOATS: [u32; 12] = [
    0x0000_0000, // +0.0
    0x8000_0000, // -0.0
    0x3F80_0000, // 1.0
    0xBF80_0000, // -1.0
    0x3F80_0001, // 1 + ulp
    0x4020_0000, // 2.5
    0x7F80_0000, // +inf
    0xFF80_0000, // -inf
    0x7FC0_0000, // qNaN
    0x7F80_0001, // sNaN
    0x0000_0001, // +subnormal
    0x4F00_0000, // 2^31
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(32))]

    #[test]
    fn fuzz_rv32f_parity(
        seeds in prop::collection::vec(0usize..12, 4),
        ops in prop::collection::vec((0u8..16, 1u8..6, 1u8..6, 1u8..6), 4..16),
    ) {
        let mut w = Vec::new();
        for (i, &s_idx) in seeds.iter().enumerate() {
            load_f32_into(FReg((i + 1) as u8), Reg(5), EDGE_FLOATS[s_idx], &mut w);
        }
        for (k, d, a, b) in ops {
            let (fd, fa, fb) = (FReg(d), FReg(a), FReg(b));
            let inst = match k {
                0 => Instruction::FaddS { rd: fd, rs1: fa, rs2: fb, rm: 7 },
                1 => Instruction::FsubS { rd: fd, rs1: fa, rs2: fb, rm: 7 },
                2 => Instruction::FmulS { rd: fd, rs1: fa, rs2: fb, rm: 7 },
                3 => Instruction::FdivS { rd: fd, rs1: fa, rs2: fb, rm: 7 },
                4 => Instruction::FsqrtS { rd: fd, rs1: fa, rm: 7 },
                5 => Instruction::FminS { rd: fd, rs1: fa, rs2: fb },
                6 => Instruction::FmaxS { rd: fd, rs1: fa, rs2: fb },
                7 => Instruction::FsgnjS { rd: fd, rs1: fa, rs2: fb },
                8 => Instruction::FsgnjnS { rd: fd, rs1: fa, rs2: fb },
                9 => Instruction::FsgnjxS { rd: fd, rs1: fa, rs2: fb },
                10 => Instruction::FmaddS { rd: fd, rs1: fa, rs2: fb, rs3: FReg(1), rm: 7 },
                11 => Instruction::FmsubS { rd: fd, rs1: fa, rs2: fb, rs3: FReg(2), rm: 7 },
                12 => Instruction::FcvtWS { rd: Reg(6), rs1: fa, rm: 7 },
                13 => Instruction::FcvtWuS { rd: Reg(7), rs1: fa, rm: 7 },
                14 => Instruction::FclassS { rd: Reg(8), rs1: fa },
                _ => Instruction::FeqS { rd: Reg(9), rs1: fa, rs2: fb },
            };
            w.push(encode_instruction(&inst));
        }
        w.extend_from_slice(&exit_seq(0));
        assert_rv32f_parity(&w, &OptConfig::o0());
        assert_rv32f_parity(&w, &OptConfig::o2());
    }
}
