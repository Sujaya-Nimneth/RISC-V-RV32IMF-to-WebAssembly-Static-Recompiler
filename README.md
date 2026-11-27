# RISC-V (RV32IMF) to WebAssembly Static Binary Recompiler

A zero-backend, Ahead-of-Time (AOT) static binary recompiler written in Rust that lifts unmodified 32-bit RISC-V (`RV32IM + F` single-precision floating point) static ELF executables into validated, optimized, standalone WebAssembly (`.wasm`) modules—running natively via CLI (`Wasmtime`) and 100% client-side in the browser (`wasm32-unknown-unknown`).

## Key Capabilities

- **Full RV32IM + RV32F + Zicsr Support**: Decodes, disassembles, interprets, lifts, optimizes, and recompiles all 40 RV32I, 8 RV32M, and 26 RV32F instructions plus FP CSRs (`fflags`, `frm`, `fcsr`) with exact RISC-V semantics (`DIV`/`REM` by zero & `INT_MIN / -1`, `FMIN.S`/`FMAX.S` non-NaN selection & `-0.0 < +0.0`, `FCVT` NaN saturation, exact single-rounding `FMADD.S`, canonical quiet NaN `0x7FC0_0000`).
- **Toggleable SSA Optimization Pipeline (`-O0` / `-O2`)**: Zero-register (`x0`) elimination, Braun SSA register promotion (`x1–x31` and `f0–f31`), constant & `LUI`/`AUIPC + ADDI` folding, safe non-negative `memarg.offset` folding, compare-and-branch fusion, Dead Code Elimination (DCE), and block-local Wasm local coalescing.
- **Structured Control Flow & Tail-Call Dispatch**: Stackifier CFG recovery with an irreducible-CFG state-machine dispatcher fallback, plus a dense `funcref` dispatch table using Wasm tail calls (`return_call` / `return_call_indirect`).
- **Differential Correctness Oracle & Fuzzing**: Spec-accurate `rv-interp` reference interpreter + `proptest` differential fuzzing asserting bit-identical `x0–x31`, `f0–f31` IEEE-754 bits, linear memory CRC32, and `stdout`.
- **100% Client-Side Web Demo & 3-Pane Compiler Explorer (`web/`)**: Compiles `rv-web-bridge` to WebAssembly (`wasm-bindgen 0.2.130`) with 6 pre-compiled presets, custom `.elf` upload (up to 4 MB), live `<canvas>` + Speedup Race, 3-pane RISC-V Disassembly / SSA IR / Emitted WAT Explorer, interactive SVG CFG Graph Viewer, and standalone `.wasm`/`.wat` export.

## Virtual Hardware ABI (Single Wasm Linear Memory)

- **`0x0000_0000 – 0x00FF_FFFF` (16 MB)**: Guest `.text` (`0x0001_0000`), `.rodata`, `.data`, `.bss`, upward bump heap (`sys_sbrk`), downward stack (`sp = 0x00FF_FFF0`).
- **`0x0100_0000 – 0x0112_BFFF`**: Memory-mapped RGBA8888 framebuffer (`320×200` or `640×480`).
- **`0x0120_0000 – 0x0120_0FFF`**: MMIO input/clock/display registers (keyboard bitmasks, mouse X/Y/buttons, monotonic ms clock, framebuffer dimensions, frame count).
- **`ecall` Syscalls (`a7`)**: `1` `sys_write`, `2` `sys_exit`, `3` `sys_clock_ms`, `4` `sys_present_frame`, `5` `sys_poll_input`, `6` `sys_sbrk`.

## Built-in Guest Presets (`web/public/presets/`)

| Preset | ISA | Description |
| :--- | :--- | :--- |
| `raycaster.elf` | `RV32IMF` | Interactive 60 FPS software 3D raycaster with textured walls, minimap & WASD/mouse look |
| `life.elf` | `RV32IM` | Conway's Game of Life in C (`Space` = pause, `R` = reseed, mouse = paint cells) |
| `mandelbrot.elf` | `RV32IMF` | `320×200` IEEE-754 single-precision floating-point fractal renderer |
| `dhrystone.elf` | `RV32IM` | Dhrystone-style integer benchmark stressing branches, pointers, structs & libc |
| `matmul.elf` | `RV32IMF` | Dual integer + FP32 `48×48` matrix multiplication benchmark |
| `compliance.elf` | `RV32IMF` | Self-checking RV32IM + RV32F edge-case compliance suite |

## Measured Benchmark Results (Wasmtime, Apple Silicon)

Generated via `cargo run --release -p recomp-cli -- bench web/public/presets/mandelbrot.elf web/public/presets/matmul.elf web/public/presets/dhrystone.elf`:

| Workload | ELF Size | -O0 Wasm | -O2 Wasm | -O2 Compile (ms) | Interp (ms) | -O0 (ms) | -O2 (ms) | -O2 MIPS | Speedup |
| :--- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| mandelbrot.elf | 8200 B | 29789 B | 27143 B | 10.10 | 84.56 | 12.29 | 12.23 | 1044.2 | 6.9x |
| matmul.elf | 8496 B | 30709 B | 27363 B | 3.73 | 8.28 | 0.96 | 0.65 | 2329.4 | 12.7x |
| dhrystone.elf | 8364 B | 30396 B | 27604 B | 3.69 | 118.27 | 5.35 | 3.23 | 7319.1 | 36.6x |

## Workspace Layout

- `crates/rv-abi` — Virtual Hardware ABI constants, MMIO layout, and `HostEnv` trait
- `crates/rv-decode` — RV32IMF decoder, disassembler, ELF32 loader (`object 0.41.0`), and CFG discovery
- `crates/rv-ir` — SSA IR and toggleable optimization passes
- `crates/rv-wasm` — Stackifier/Dispatcher CFG recovery, tail-call dispatch, `wasm-encoder 0.263.0`, `wasmparser 0.263.0`, Wasmtime runner
- `crates/rv-interp` — Spec-accurate RV32IMF reference interpreter
- `crates/rv-web-bridge` — `wasm-bindgen 0.2.130` browser bridge
- `crates/recomp-cli` — Native CLI (`compile`, `run`, `inspect`, `diff`, `bench`)
- `guests/` — Bare-metal C presets, linker script, `crt0.S`, `minilibc.c`, and `build.sh`
- `web/` — React + TypeScript + Vite static web app (with COOP/COEP headers for Vercel, Cloudflare Pages, and `coi-serviceworker.js` fallback)

## Usage

```bash
# Run all Rust compliance, differential, and fuzz tests
cargo test --workspace

# Recompile an ELF to standalone .wasm (or .wat with --wat)
cargo run --release -p recomp-cli -- compile web/public/presets/mandelbrot.elf -o mandelbrot.wasm

# Differential parity check: Interpreter vs Recompiled -O2 Wasm
cargo run --release -p recomp-cli -- diff web/public/presets/mandelbrot.elf --opt o2

# Build the 100% static browser bundle into web/dist/
./scripts/build-web.sh

# Run local web dev server
cd web && npm run dev
```
