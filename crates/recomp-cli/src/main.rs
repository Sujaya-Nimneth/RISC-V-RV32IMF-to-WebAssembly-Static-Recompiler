use clap::{Parser, Subcommand, ValueEnum};
use rv_abi::HeadlessHost;
use rv_decode::{LoadedElf, disassemble, discover_cfg};
use rv_interp::Interpreter;
use rv_ir::{OptConfig, lift_program, print_function};
use rv_wasm::{compile_to_wasm, runtime::execute_wasm};
use std::path::PathBuf;
use std::time::Instant;

#[derive(Parser)]
#[command(name = "recomp-cli", about = "RISC-V (RV32IMF) to WebAssembly Static Binary Recompiler")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Clone, Copy, ValueEnum)]
enum OptLevel {
    O0,
    O2,
}

#[derive(Clone, Copy, ValueEnum)]
enum RunMode {
    Interp,
    Recomp,
}

#[derive(Subcommand)]
enum Cmd {
    /// Recompile an ELF32 RISC-V binary to standalone .wasm
    Compile {
        input: PathBuf,
        #[arg(short, long)]
        output: PathBuf,
        #[arg(long, value_enum, default_value_t = OptLevel::O2)]
        opt: OptLevel,
        #[arg(long)]
        wat: bool,
    },
    /// Execute an ELF32 RISC-V binary via interpreter or recompiled Wasm
    Run {
        input: PathBuf,
        #[arg(long, value_enum, default_value_t = RunMode::Recomp)]
        mode: RunMode,
        #[arg(long, value_enum, default_value_t = OptLevel::O2)]
        opt: OptLevel,
        #[arg(long, default_value_t = 50_000_000)]
        max_steps: u64,
    },
    /// Inspect discovered functions, disassembly, SSA IR, and WAT
    Inspect {
        input: PathBuf,
        #[arg(long)]
        fn_name: Option<String>,
        #[arg(long, value_enum, default_value_t = OptLevel::O2)]
        opt: OptLevel,
    },
    /// Differential parity check: Interpreter vs Recompiled Wasm
    Diff {
        input: PathBuf,
        #[arg(long, value_enum, default_value_t = OptLevel::O0)]
        opt: OptLevel,
    },
    /// Benchmark Interpreter vs Recompiled -O0 vs Recompiled -O2
    Bench {
        inputs: Vec<PathBuf>,
    },
}

fn opt_from_level(opt: OptLevel) -> OptConfig {
    match opt {
        OptLevel::O0 => OptConfig::o0(),
        OptLevel::O2 => OptConfig::o2(),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.cmd {
        Cmd::Compile {
            input,
            output,
            opt,
            wat,
        } => {
            let bytes = std::fs::read(&input)?;
            let elf = LoadedElf::parse(&bytes)?;
            let cfg = discover_cfg(&elf)?;
            let ir = lift_program(&cfg, &opt_from_level(opt));
            let compiled = compile_to_wasm(&ir, &elf)?;
            if wat {
                let wat_text = wasmprinter::print_bytes(&compiled.wasm_bytes)?;
                std::fs::write(&output, wat_text)?;
            } else {
                std::fs::write(&output, &compiled.wasm_bytes)?;
            }
            println!(
                "Compiled {} -> {} ({} bytes, {} fns, valid={})",
                input.display(),
                output.display(),
                compiled.wasm_bytes.len(),
                compiled.function_count,
                compiled.valid
            );
        }
        Cmd::Run {
            input,
            mode,
            opt,
            max_steps,
        } => {
            let bytes = std::fs::read(&input)?;
            let elf = LoadedElf::parse(&bytes)?;
            match mode {
                RunMode::Interp => {
                    let mut interp = Interpreter::from_elf(&elf);
                    let mut host = HeadlessHost::new();
                    interp.run(&mut host, max_steps)?;
                    print!("{}", host.stdout_str());
                    eprint!("{}", host.stderr_str());
                    println!(
                        "[interp] exit={:?} instret={}",
                        host.exit_code, interp.cpu.instret
                    );
                }
                RunMode::Recomp => {
                    let cfg = discover_cfg(&elf)?;
                    let ir = lift_program(&cfg, &opt_from_level(opt));
                    let compiled = compile_to_wasm(&ir, &elf)?;
                    let (res, host) = execute_wasm(&compiled.wasm_bytes, HeadlessHost::new())?;
                    print!("{}", host.stdout_str());
                    eprint!("{}", host.stderr_str());
                    println!(
                        "[recomp] exit={} instret={}",
                        res.exit_code, res.instret
                    );
                }
            }
        }
        Cmd::Inspect {
            input,
            fn_name,
            opt,
        } => {
            let bytes = std::fs::read(&input)?;
            let elf = LoadedElf::parse(&bytes)?;
            let cfg = discover_cfg(&elf)?;
            let ir = lift_program(&cfg, &opt_from_level(opt));
            for (&addr, fcfg) in &cfg.functions {
                if let Some(ref filter) = fn_name {
                    if !fcfg.name.contains(filter) {
                        continue;
                    }
                }
                println!("=== {} (0x{:08x}) ===", fcfg.name, addr);
                for bb in fcfg.blocks.values() {
                    for di in &bb.insts {
                        println!("  0x{:08x}: {}", di.pc, disassemble(di.pc, &di.inst));
                    }
                }
                if let Some(ir_fn) = ir.functions.get(&addr) {
                    println!("{}", print_function(ir_fn));
                }
            }
        }
        Cmd::Diff { input, opt } => {
            let bytes = std::fs::read(&input)?;
            let elf = LoadedElf::parse(&bytes)?;
            let cfg = discover_cfg(&elf)?;
            let ir = lift_program(&cfg, &opt_from_level(opt));
            let compiled = compile_to_wasm(&ir, &elf)?;

            let mut interp = Interpreter::from_elf(&elf);
            let mut interp_host = HeadlessHost::new();
            interp.run(&mut interp_host, 50_000_000)?;

            let (wasm_res, wasm_host) = execute_wasm(&compiled.wasm_bytes, HeadlessHost::new())?;

            assert_eq!(
                interp_host.exit_code,
                Some(wasm_res.exit_code),
                "exit code mismatch"
            );
            assert_eq!(
                interp_host.stdout_str(),
                wasm_host.stdout_str(),
                "stdout mismatch"
            );
            for r in 0..32 {
                assert_eq!(
                    interp.cpu.x[r], wasm_res.regs[r],
                    "register x{r} mismatch"
                );
            }
            assert_eq!(
                interp.memory_crc32(),
                wasm_res.memory_crc32,
                "memory CRC32 mismatch"
            );
            println!(
                "DIFF PASS: {} (exit={}, crc32=0x{:08x}, instret={})",
                input.display(),
                wasm_res.exit_code,
                wasm_res.memory_crc32,
                wasm_res.instret
            );
        }
        Cmd::Bench { inputs } => {
            println!("| Workload | ELF Size | -O0 Wasm | -O2 Wasm | -O2 Compile (ms) | Interp (ms) | -O0 (ms) | -O2 (ms) | -O2 MIPS | Speedup |");
            println!("| :--- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |");
            for path in inputs {
                let bytes = std::fs::read(&path)?;
                let elf = LoadedElf::parse(&bytes)?;
                let cfg = discover_cfg(&elf)?;

                let ir_o0 = lift_program(&cfg, &OptConfig::o0());
                let wasm_o0 = compile_to_wasm(&ir_o0, &elf)?;

                let t_c0 = Instant::now();
                let ir_o2 = lift_program(&cfg, &OptConfig::o2());
                let wasm_o2 = compile_to_wasm(&ir_o2, &elf)?;
                let compile_ms = t_c0.elapsed().as_secs_f64() * 1000.0;

                let t_i0 = Instant::now();
                let mut interp = Interpreter::from_elf(&elf);
                let mut i_host = HeadlessHost::new();
                interp.run(&mut i_host, 200_000_000)?;
                let interp_ms = t_i0.elapsed().as_secs_f64() * 1000.0;

                let t_w0 = Instant::now();
                let (_, _) = execute_wasm(&wasm_o0.wasm_bytes, HeadlessHost::new())?;
                let o0_ms = t_w0.elapsed().as_secs_f64() * 1000.0;

                let t_w2 = Instant::now();
                let (res_o2, _) = execute_wasm(&wasm_o2.wasm_bytes, HeadlessHost::new())?;
                let o2_ms = t_w2.elapsed().as_secs_f64() * 1000.0;

                let mips = (res_o2.instret as f64) / (o2_ms * 1000.0).max(1e-6);
                let speedup = interp_ms / o2_ms.max(1e-6);
                let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("elf");
                println!(
                    "| {name} | {} B | {} B | {} B | {compile_ms:.2} | {interp_ms:.2} | {o0_ms:.2} | {o2_ms:.2} | {mips:.1} | {speedup:.1}x |",
                    bytes.len(),
                    wasm_o0.wasm_bytes.len(),
                    wasm_o2.wasm_bytes.len()
                );
            }
        }
    }
    Ok(())
}
