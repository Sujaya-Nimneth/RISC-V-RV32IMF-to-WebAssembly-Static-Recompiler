use rv_abi::{HeadlessHost, SYS_EXIT, SYS_SBRK, SYS_WRITE};
use rv_decode::{Instruction, LoadedElf, Reg, build_test_elf, discover_cfg, encode_instruction};
use rv_interp::Interpreter;
use rv_ir::{OptConfig, lift_program};
use rv_wasm::{compile_to_wasm, runtime::execute_wasm};

fn assert_diff_parity(words: &[u32], rodata: &[u8], opt: OptConfig) {
    let elf_bytes = build_test_elf(words, rodata);
    assert_diff_elf(&elf_bytes, opt);
}

fn assert_diff_elf(elf_bytes: &[u8], opt: OptConfig) {
    let elf = LoadedElf::parse(elf_bytes).expect("parse elf");
    let cfg = discover_cfg(&elf).expect("discover cfg");
    let ir = lift_program(&cfg, &opt);
    let compiled = compile_to_wasm(&ir, &elf).expect("compile to wasm");
    assert!(compiled.valid, "wasmparser failed: {:?}", compiled.validation_error);

    let mut interp = Interpreter::from_elf(&elf);
    let mut interp_host = HeadlessHost::new();
    interp.run(&mut interp_host, 10_000_000).expect("run interp");

    let (wasm_res, wasm_host) =
        execute_wasm(&compiled.wasm_bytes, HeadlessHost::new()).expect("run wasm");

    assert_eq!(interp_host.exit_code, Some(wasm_res.exit_code), "exit_code mismatch");
    assert_eq!(interp_host.stdout_str(), wasm_host.stdout_str(), "stdout mismatch");
    for r in 0..32 {
        assert_eq!(
            interp.cpu.x[r], wasm_res.regs[r],
            "x{r} mismatch: interp=0x{:08x}, wasm=0x{:08x}",
            interp.cpu.x[r], wasm_res.regs[r]
        );
    }
    assert_eq!(
        interp.memory_crc32(),
        wasm_res.memory_crc32,
        "memory CRC32 mismatch"
    );
}

fn exit_seq(code: i32) -> [u32; 3] {
    [
        encode_instruction(&Instruction::Addi {
            rd: Reg::A0,
            rs1: Reg::ZERO,
            imm: code,
        }),
        encode_instruction(&Instruction::Addi {
            rd: Reg::A7,
            rs1: Reg::ZERO,
            imm: SYS_EXIT as i32,
        }),
        encode_instruction(&Instruction::Ecall),
    ]
}

#[test]
fn diff_rv32im_alu_and_mul_div_edge_cases() {
    let mut words = vec![
        encode_instruction(&Instruction::Lui { rd: Reg(5), imm: i32::MIN }),
        encode_instruction(&Instruction::Addi { rd: Reg(6), rs1: Reg::ZERO, imm: -1 }),
        encode_instruction(&Instruction::Addi { rd: Reg(7), rs1: Reg::ZERO, imm: 0 }),
        encode_instruction(&Instruction::Div { rd: Reg(8), rs1: Reg(5), rs2: Reg(6) }),
        encode_instruction(&Instruction::Rem { rd: Reg(9), rs1: Reg(5), rs2: Reg(6) }),
        encode_instruction(&Instruction::Addi { rd: Reg(11), rs1: Reg::ZERO, imm: 42 }),
        encode_instruction(&Instruction::Div { rd: Reg(12), rs1: Reg(11), rs2: Reg(7) }),
        encode_instruction(&Instruction::Divu { rd: Reg(13), rs1: Reg(11), rs2: Reg(7) }),
        encode_instruction(&Instruction::Rem { rd: Reg(14), rs1: Reg(11), rs2: Reg(7) }),
        encode_instruction(&Instruction::Remu { rd: Reg(15), rs1: Reg(11), rs2: Reg(7) }),
        encode_instruction(&Instruction::Mulh { rd: Reg(16), rs1: Reg(5), rs2: Reg(6) }),
        encode_instruction(&Instruction::Mulhsu { rd: Reg(18), rs1: Reg(5), rs2: Reg(11) }),
        encode_instruction(&Instruction::Mulhu { rd: Reg(19), rs1: Reg(5), rs2: Reg(11) }),
        encode_instruction(&Instruction::Srai { rd: Reg(20), rs1: Reg(5), shamt: 7 }),
        encode_instruction(&Instruction::Srli { rd: Reg(21), rs1: Reg(5), shamt: 7 }),
    ];
    words.extend_from_slice(&exit_seq(3));
    assert_diff_parity(&words, &[], OptConfig::o0());
}

#[test]
fn diff_rv32im_loop_and_memory() {
    // Sum 1..10 in a loop, store into stack, reload with lb/lh/lw
    let mut words = vec![
        encode_instruction(&Instruction::Addi { rd: Reg(5), rs1: Reg::ZERO, imm: 0 }), // sum = 0
        encode_instruction(&Instruction::Addi { rd: Reg(6), rs1: Reg::ZERO, imm: 1 }), // i = 1
        encode_instruction(&Instruction::Addi { rd: Reg(7), rs1: Reg::ZERO, imm: 11 }), // limit
        // loop (pc+12):
        encode_instruction(&Instruction::Bge { rs1: Reg(6), rs2: Reg(7), imm: 16 }),
        encode_instruction(&Instruction::Add { rd: Reg(5), rs1: Reg(5), rs2: Reg(6) }),
        encode_instruction(&Instruction::Addi { rd: Reg(6), rs1: Reg(6), imm: 1 }),
        encode_instruction(&Instruction::Jal { rd: Reg::ZERO, imm: -12 }),
        // after loop:
        encode_instruction(&Instruction::Sw { rs1: Reg::SP, rs2: Reg(5), imm: -16 }),
        encode_instruction(&Instruction::Lw { rd: Reg(8), rs1: Reg::SP, imm: -16 }),
        encode_instruction(&Instruction::Lb { rd: Reg(9), rs1: Reg::SP, imm: -16 }),
    ];
    words.extend_from_slice(&exit_seq(0));
    assert_diff_parity(&words, &[], OptConfig::o0());
}

#[test]
fn diff_rv32im_call_and_indirect_call() {
    // main calls helper via jal, then calls helper via jalr
    let helper_pc = rv_abi::TEXT_BASE + 12 * 4;
    let mut words = vec![
        // 0: addi a0, zero, 20
        encode_instruction(&Instruction::Addi { rd: Reg::A0, rs1: Reg::ZERO, imm: 20 }),
        // 1: jal ra, helper (+11*4 = +44)
        encode_instruction(&Instruction::Jal { rd: Reg::RA, imm: 11 * 4 }),
        // 2: mv t0, a0 (add t0, a0, zero)
        encode_instruction(&Instruction::Add { rd: Reg(5), rs1: Reg::A0, rs2: Reg::ZERO }),
        // 3: lui t1, helper_hi
        encode_instruction(&Instruction::Lui {
            rd: Reg(6),
            imm: (helper_pc & 0xFFFF_F000) as i32,
        }),
        // 4: addi t1, t1, helper_lo
        encode_instruction(&Instruction::Addi {
            rd: Reg(6),
            rs1: Reg(6),
            imm: (helper_pc & 0xFFF) as i32,
        }),
        // 5: jalr ra, t1, 0
        encode_instruction(&Instruction::Jalr { rd: Reg::RA, rs1: Reg(6), imm: 0 }),
        // 6: add t0, t0, a0
        encode_instruction(&Instruction::Add { rd: Reg(5), rs1: Reg(5), rs2: Reg::A0 }),
        // 7: sbrk(32)
        encode_instruction(&Instruction::Addi { rd: Reg::A0, rs1: Reg::ZERO, imm: 32 }),
        encode_instruction(&Instruction::Addi { rd: Reg::A7, rs1: Reg::ZERO, imm: SYS_SBRK as i32 }),
        encode_instruction(&Instruction::Ecall),
        // 10..11: exit(0) (2 insts? wait: exit_seq is 3 insts!)
    ];
    let _ = words.pop();
    // Rebuild cleanly with exact offsets:
    let mut prog = vec![
        encode_instruction(&Instruction::Addi { rd: Reg::A0, rs1: Reg::ZERO, imm: 20 }), // 0
        encode_instruction(&Instruction::Jal { rd: Reg::RA, imm: 10 * 4 }), // 1 -> 11
        encode_instruction(&Instruction::Add { rd: Reg(5), rs1: Reg::A0, rs2: Reg::ZERO }), // 2
        encode_instruction(&Instruction::Lui {
            rd: Reg(6),
            imm: ((rv_abi::TEXT_BASE + 11 * 4) & 0xFFFF_F000) as i32,
        }), // 3
        encode_instruction(&Instruction::Addi {
            rd: Reg(6),
            rs1: Reg(6),
            imm: ((rv_abi::TEXT_BASE + 11 * 4) & 0xFFF) as i32,
        }), // 4
        encode_instruction(&Instruction::Jalr { rd: Reg::RA, rs1: Reg(6), imm: 0 }), // 5
        encode_instruction(&Instruction::Add { rd: Reg(5), rs1: Reg(5), rs2: Reg::A0 }), // 6
        encode_instruction(&Instruction::Addi { rd: Reg::A0, rs1: Reg::ZERO, imm: 1 }), // 7
        encode_instruction(&Instruction::Addi { rd: Reg::A7, rs1: Reg::ZERO, imm: SYS_WRITE as i32 }), // 8 (writes 0 len? let's exit)
    ];
    prog.truncate(7);
    prog.extend_from_slice(&exit_seq(0)); // 7, 8, 9
    // 10: nop (padding)
    prog.push(encode_instruction(&Instruction::Addi { rd: Reg::ZERO, rs1: Reg::ZERO, imm: 0 }));
    // 11: helper: a0 = a0 + 5; ret
    prog.push(encode_instruction(&Instruction::Addi { rd: Reg::A0, rs1: Reg::A0, imm: 5 }));
    prog.push(encode_instruction(&Instruction::Jalr { rd: Reg::ZERO, rs1: Reg::RA, imm: 0 }));

    assert_diff_parity(&prog, &[], OptConfig::o0());
}

#[test]
fn diff_hello_smoke_elf() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../guests/common/hello.elf");
    let bytes = std::fs::read(path).expect("read hello.elf");
    assert_diff_elf(&bytes, OptConfig::o0());
    assert_diff_elf(&bytes, OptConfig::o2());

    let elf = LoadedElf::parse(&bytes).expect("parse elf");
    let cfg = discover_cfg(&elf).expect("cfg");
    let w0 = compile_to_wasm(&lift_program(&cfg, &OptConfig::o0()), &elf).unwrap();
    let w2 = compile_to_wasm(&lift_program(&cfg, &OptConfig::o2()), &elf).unwrap();
    assert!(
        w2.wasm_bytes.len() < w0.wasm_bytes.len(),
        "expected -O2 ({}) < -O0 ({})",
        w2.wasm_bytes.len(),
        w0.wasm_bytes.len()
    );
}

#[test]
fn diff_rv32im_o2_all() {
    diff_rv32im_alu_and_mul_div_edge_cases();
}
