use rv_abi::{HeadlessHost, SYS_EXIT, SYS_SBRK, SYS_WRITE};
use rv_decode::{Instruction, LoadedElf, Reg, build_test_elf, discover_cfg, encode_instruction};
use rv_interp::Interpreter;

fn run_words(words: &[u32], rodata: &[u8]) -> (Interpreter, HeadlessHost) {
    let elf_bytes = build_test_elf(words, rodata);
    let elf = LoadedElf::parse(&elf_bytes).expect("parse test elf");
    let mut interp = Interpreter::from_elf(&elf);
    let mut host = HeadlessHost::new();
    interp.run(&mut host, 100_000).expect("run interp");
    (interp, host)
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
fn test_x0_immutability_and_alu_imms() {
    let mut words = vec![
        encode_instruction(&Instruction::Addi { rd: Reg::ZERO, rs1: Reg::ZERO, imm: 123 }),
        encode_instruction(&Instruction::Lui { rd: Reg::ZERO, imm: 0x1234_5000 }),
        encode_instruction(&Instruction::Addi { rd: Reg(5), rs1: Reg::ZERO, imm: -10 }),
        encode_instruction(&Instruction::Slti { rd: Reg(6), rs1: Reg(5), imm: -5 }),
        encode_instruction(&Instruction::Sltiu { rd: Reg(7), rs1: Reg(5), imm: 5 }),
        encode_instruction(&Instruction::Xori { rd: Reg(8), rs1: Reg(5), imm: -1 }),
        encode_instruction(&Instruction::Ori { rd: Reg(9), rs1: Reg::ZERO, imm: 0x55 }),
        encode_instruction(&Instruction::Andi { rd: Reg(20), rs1: Reg(9), imm: 0x0F }),
        encode_instruction(&Instruction::Slli { rd: Reg(11), rs1: Reg(20), shamt: 4 }),
        encode_instruction(&Instruction::Srli { rd: Reg(12), rs1: Reg(5), shamt: 28 }),
        encode_instruction(&Instruction::Srai { rd: Reg(13), rs1: Reg(5), shamt: 1 }),
    ];
    words.extend_from_slice(&exit_seq(0));

    let (interp, _) = run_words(&words, &[]);
    assert_eq!(interp.reg(Reg::ZERO), 0);
    assert_eq!(interp.reg(Reg(5)) as i32, -10);
    assert_eq!(interp.reg(Reg(6)), 1);
    assert_eq!(interp.reg(Reg(7)), 0); // -10u > 5u
    assert_eq!(interp.reg(Reg(8)), !((-10i32) as u32));
    assert_eq!(interp.reg(Reg(9)), 0x55);
    assert_eq!(interp.reg(Reg(20)), 0x05);
    assert_eq!(interp.reg(Reg(11)), 0x50);
    assert_eq!(interp.reg(Reg(12)), 0xF);
    assert_eq!(interp.reg(Reg(13)) as i32, -5);
}

#[test]
fn test_rv32m_div_rem_edge_cases_and_mulh() {
    let mut words = vec![
        // x5 = INT_MIN (0x8000_0000), x6 = -1, x7 = 0
        encode_instruction(&Instruction::Lui { rd: Reg(5), imm: i32::MIN }),
        encode_instruction(&Instruction::Addi { rd: Reg(6), rs1: Reg::ZERO, imm: -1 }),
        encode_instruction(&Instruction::Addi { rd: Reg(7), rs1: Reg::ZERO, imm: 0 }),
        // DIV(INT_MIN, -1) == INT_MIN
        encode_instruction(&Instruction::Div { rd: Reg(8), rs1: Reg(5), rs2: Reg(6) }),
        // REM(INT_MIN, -1) == 0
        encode_instruction(&Instruction::Rem { rd: Reg(9), rs1: Reg(5), rs2: Reg(6) }),
        // DIV(42, 0) == -1
        encode_instruction(&Instruction::Addi { rd: Reg(10), rs1: Reg::ZERO, imm: 42 }),
        encode_instruction(&Instruction::Div { rd: Reg(11), rs1: Reg(10), rs2: Reg(7) }),
        // DIVU(42, 0) == u32::MAX
        encode_instruction(&Instruction::Divu { rd: Reg(12), rs1: Reg(10), rs2: Reg(7) }),
        // REM(42, 0) == 42, REMU(42, 0) == 42
        encode_instruction(&Instruction::Rem { rd: Reg(13), rs1: Reg(10), rs2: Reg(7) }),
        encode_instruction(&Instruction::Remu { rd: Reg(14), rs1: Reg(10), rs2: Reg(7) }),
        // MULH, MULHSU, MULHU with (-2 * -3), (-2 * 3u), (0x8000_0000 * 2)
        encode_instruction(&Instruction::Addi { rd: Reg(15), rs1: Reg::ZERO, imm: -2 }),
        encode_instruction(&Instruction::Addi { rd: Reg(16), rs1: Reg::ZERO, imm: 3 }),
        encode_instruction(&Instruction::Mulh { rd: Reg(18), rs1: Reg(15), rs2: Reg(16) }),
        encode_instruction(&Instruction::Mulhsu { rd: Reg(19), rs1: Reg(15), rs2: Reg(16) }),
        encode_instruction(&Instruction::Mulhu { rd: Reg(20), rs1: Reg(5), rs2: Reg(16) }),
    ];
    words.extend_from_slice(&exit_seq(7));

    let (interp, host) = run_words(&words, &[]);
    assert_eq!(host.exit_code, Some(7));
    assert_eq!(interp.reg(Reg(8)), i32::MIN as u32);
    assert_eq!(interp.reg(Reg(9)), 0);
    assert_eq!(interp.reg(Reg(11)), u32::MAX);
    assert_eq!(interp.reg(Reg(12)), u32::MAX);
    assert_eq!(interp.reg(Reg(13)), 42);
    assert_eq!(interp.reg(Reg(14)), 42);
    assert_eq!(interp.reg(Reg(18)) as i32, -1); // (-2 * 3) = -6 => high word -1
    assert_eq!(interp.reg(Reg(19)) as i32, -1);
    assert_eq!(interp.reg(Reg(20)), 1); // 0x8000_0000 * 3 = 0x1_8000_0000 => high word 1
}

#[test]
fn test_jalr_aliasing_and_lsb_mask() {
    // jalr x5, x5, 1 where x5 points to target+0 (odd address cleared to even)
    let target_pc = rv_abi::TEXT_BASE + 16;
    let mut words = vec![
        encode_instruction(&Instruction::Lui { rd: Reg(5), imm: (target_pc & 0xFFFF_F000) as i32 }),
        encode_instruction(&Instruction::Addi {
            rd: Reg(5),
            rs1: Reg(5),
            imm: (target_pc & 0xFFF) as i32,
        }),
        encode_instruction(&Instruction::Jalr { rd: Reg(5), rs1: Reg(5), imm: 1 }), // pc = BASE+8
        encode_instruction(&Instruction::Addi { rd: Reg(6), rs1: Reg::ZERO, imm: 999 }), // skipped
        // target_pc (BASE+16):
        encode_instruction(&Instruction::Addi { rd: Reg(6), rs1: Reg::ZERO, imm: 123 }),
    ];
    words.extend_from_slice(&exit_seq(0));

    let (interp, _) = run_words(&words, &[]);
    assert_eq!(interp.reg(Reg(5)), rv_abi::TEXT_BASE + 12);
    assert_eq!(interp.reg(Reg(6)), 123);
}

#[test]
fn test_unaligned_loads_stores_and_sign_ext() {
    let mut words = vec![
        encode_instruction(&Instruction::Addi { rd: Reg(5), rs1: Reg::SP, imm: -32 }),
        encode_instruction(&Instruction::Lui { rd: Reg(6), imm: 0x807F_1000u32 as i32 }),
        encode_instruction(&Instruction::Addi { rd: Reg(6), rs1: Reg(6), imm: -127 }),
        // Unaligned store at sp - 31
        encode_instruction(&Instruction::Sw { rs1: Reg(5), rs2: Reg(6), imm: 1 }),
        encode_instruction(&Instruction::Lw { rd: Reg(7), rs1: Reg(5), imm: 1 }),
        encode_instruction(&Instruction::Lb { rd: Reg(8), rs1: Reg(5), imm: 1 }),
        encode_instruction(&Instruction::Lbu { rd: Reg(9), rs1: Reg(5), imm: 1 }),
        encode_instruction(&Instruction::Lh { rd: Reg(20), rs1: Reg(5), imm: 3 }),
        encode_instruction(&Instruction::Lhu { rd: Reg(11), rs1: Reg(5), imm: 3 }),
    ];
    words.extend_from_slice(&exit_seq(0));

    let (interp, _) = run_words(&words, &[]);
    let stored = 0x807F_1000u32.wrapping_add((-127i32) as u32);
    assert_eq!(interp.reg(Reg(7)), stored);
    assert_eq!(interp.reg(Reg(8)) as i32, (stored as u8) as i8 as i32);
    assert_eq!(interp.reg(Reg(9)), (stored as u8) as u32);
    assert_eq!(interp.reg(Reg(20)) as i32, ((stored >> 16) as u16) as i16 as i32);
    assert_eq!(interp.reg(Reg(11)), ((stored >> 16) as u16) as u32);
}

#[test]
fn test_sys_write_and_sbrk() {
    let msg = b"RV32IM_OK";
    let mut words = vec![
        // sbrk(64) -> x5
        encode_instruction(&Instruction::Addi { rd: Reg::A0, rs1: Reg::ZERO, imm: 64 }),
        encode_instruction(&Instruction::Addi { rd: Reg::A7, rs1: Reg::ZERO, imm: SYS_SBRK as i32 }),
        encode_instruction(&Instruction::Ecall),
        encode_instruction(&Instruction::Add { rd: Reg(5), rs1: Reg::A0, rs2: Reg::ZERO }),
        // write(1, rodata, 9)
        encode_instruction(&Instruction::Addi { rd: Reg::A0, rs1: Reg::ZERO, imm: 1 }),
        // rodata_vaddr = TEXT_BASE + 13*4 + 0x100
        encode_instruction(&Instruction::Lui { rd: Reg::A1, imm: 0x0001_0000 }),
        encode_instruction(&Instruction::Addi { rd: Reg::A1, rs1: Reg::A1, imm: 13 * 4 + 0x100 }),
        encode_instruction(&Instruction::Addi { rd: Reg::A2, rs1: Reg::ZERO, imm: msg.len() as i32 }),
        encode_instruction(&Instruction::Addi { rd: Reg::A7, rs1: Reg::ZERO, imm: SYS_WRITE as i32 }),
        encode_instruction(&Instruction::Ecall),
    ];
    words.extend_from_slice(&exit_seq(0));

    let (interp, host) = run_words(&words, msg);
    assert_eq!(host.stdout_str(), "RV32IM_OK");
    assert_eq!(host.exit_code, Some(0));
    assert!(interp.reg(Reg(5)) >= rv_abi::TEXT_BASE);
    assert_eq!(interp.heap_brk, interp.reg(Reg(5)) + 64);
}

#[test]
fn test_hello_smoke_elf_cfg_and_interp() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../guests/common/hello.elf");
    let bytes = std::fs::read(path).expect("read hello.elf");
    let elf = LoadedElf::parse(&bytes).expect("parse hello.elf");
    let cfg = discover_cfg(&elf).expect("discover cfg");
    assert!(!cfg.functions.is_empty());

    let mut interp = Interpreter::from_elf(&elf);
    let mut host = HeadlessHost::new();
    interp.run(&mut host, 1_000_000).expect("run hello.elf");
    assert_eq!(host.exit_code, Some(0));
    assert_eq!(host.stdout_str(), "Hello from RV32IM! sum(1..10) = 55\n");
}
