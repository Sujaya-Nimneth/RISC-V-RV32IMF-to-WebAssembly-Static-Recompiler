# RISC-V (RV32IMF) to WebAssembly Static Binary Recompiler

A zero-backend, Ahead-of-Time (AOT) static binary recompiler written in Rust that lifts unmodified 32-bit RISC-V (`RV32IM + F` single-precision floating point) static ELF executables into validated, optimized, standalone WebAssembly (`.wasm`) modules—running natively via CLI (`Wasmtime`) and 100% client-side in the browser (`wasm32-unknown-unknown`).

## Features

- **Full RV32IM + RV32F + Zicsr Support**: Decodes, interprets, lifts, optimizes, and recompiles all 40 RV32I, 8 RV32M, and 26 RV32F instructions + FP CSRs (`fflags`, `frm`, `fcsr`) with exact RISC-V semantics (`DIV`/`REM` by zero & `INT_MIN / -1`, `FMIN`/`FMAX` non-NaN & `-0.0 < +0.0`, `FCVT` NaN saturation, exact single-rounding `FMADD.S`, canonical quiet NaN `0x7FC0_0000`).
- **Toggleable SSA Optimization Pipeline (`-O0` / `-O2`)**: Zero-register elimination, Braun SSA register promotion (`x1–x31`, `f0–f31`), constant & `LUI`/`AUIPC + ADDI` folding, safe non-negative `memarg.offset` folding, compare-and-branch fusion, Dead Code Elimination (DCE), and block-local Wasm local coalescing.
- **Structured Control Flow & Tail-Call Dispatch**: Stackifier CFG recovery with an irreducible-CFG state-machine dispatcher fallback, plus a dense `funcref` dispatch table using Wasm tail calls (`return_call` / `return_call_indirect`).
- **Differential Correctness Oracle & Fuzzing**: Spec-accurate `rv-interp` reference interpreter + `proptest` differential fuzzing asserting bit-identical `x0–x31`, `f0–f31` IEEE-754 bits, memory CRC32, and `stdout`.
- **Client-Side Web Demo & 3-Pane Compiler Explorer (`web/`)**: Runs 100% in the browser via `rv-web-bridge` (`wasm-bindgen 0.2.130`), featuring 6 pre-compiled presets (3D Raycaster, Game of Life, Mandelbrot, Dhrystone, MatMul, ISA Compliance), custom `.elf` drag-and-drop, live `<canvas>` + Speedup Race, 3-pane Disassembly / SSA IR / WAT Explorer, interactive SVG CFG Graph Viewer, and `.wasm`/`.wat` export.

## Measured Benchmark Results (Wasmtime, Apple Silicon)

Generated via `cargo run --release -p recomp-cli -- bench web/public/presets/mandelbrot.elf web/public/presets/matmul.elf web/public/presets/dhrystone.elf`:

| Workload | ELF Size | -O0 Wasm | -O2 Wasm | -O2 Compile (ms) | Interp (ms) | -O0 (ms) | -O2 (ms) | -O2 MIPS | Speedup |
| :--- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| mandelbrot.elf | 8200 B | 29789 B | 27143 B | 10.10 | 84.56 | 12.29 | 12.23 | 1044.2 | 6.9x |
| matmul.elf | 8496 B | 30709 B | 27363 B | 3.73 | 8.28 | 0.96 | 0.65 | 2329.4 | 12.7x |
| dhrystone.elf | 8364 B | 30396 B | 27604 B | 3.69 | 118.27 | 5.35 | 3.23 | 7319.1 | 36.6x |

## Workspace Layout

- `crates/rv-abi` — Virtual Hardware ABI (16 MB RAM, `0x0100_0000` RGBA8888 framebuffer, `0x0120_0000` MMIO, 6 `ecall` syscalls)
- `crates/rv-decode` — RV32IMF decoder, disassembler, ELF32 loader (`object 0.41.0`), recursive-descent CFG discovery
- `crates/rv-ir` — SSA IR and toggleable optimization passes
- `crates/rv-wasm` — Stackifier/Dispatcher CFG recovery, tail-call dispatch, `wasm-encoder 0.263.0`, `wasmparser 0.263.0`, Wasmtime runner
- `crates/rv-interp` — Spec-accurate RV32IMF reference interpreter
- `crates/rv-web-bridge` — `wasm-bindgen 0.2.130` browser bridge
- `crates/recomp-cli` — CLI (`compile`, `run`, `inspect`, `diff`, `bench`)
- `guests/` — Bare-metal C presets & `build.sh`
- `web/` — React + TypeScript + Vite static web app

## Quick Start

```bash
# Run all Rust compliance, differential, and fuzz tests
cargo test --workspace

# Recompile and differential-check a RISC-V ELF
cargo run --release -p recomp-cli -- diff web/public/presets/mandelbrot.elf --opt o2

# Build the 100% static browser bundle into web/dist/
./scripts/build-web.sh

# Run local dev server
cd web && npm run dev
```

See [MASTER_PLAN.md](./MASTER_PLAN.md) for the full technical architecture.
