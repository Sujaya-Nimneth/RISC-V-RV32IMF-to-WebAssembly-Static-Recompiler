# RISC-V (RV32IMF) to WebAssembly Static Binary Recompiler

A zero-backend, Ahead-of-Time (AOT) static binary recompiler written in Rust that lifts unmodified 32-bit RISC-V (`RV32IM + F`) static ELF executables into validated, optimized, standalone WebAssembly (`.wasm`) modules.

## Architecture

- **`crates/rv-abi`**: Shared Virtual Hardware ABI (16 MB RAM + RGBA8888 framebuffer + MMIO + `ecall` syscalls)
- **`crates/rv-decode`**: Pure Rust RV32IMF decoder, disassembler, ELF32 loader (`object`), and recursive-descent CFG discovery
- **`crates/rv-ir`**: SSA Intermediate Representation and toggleable optimization passes (`-O0` / `-O2`)
- **`crates/rv-wasm`**: Stackifier structured CFG recovery, tail-call `funcref` dispatch table, `wasm-encoder` emission, and `wasmparser` validation
- **`crates/rv-interp`**: Spec-accurate RV32IMF reference interpreter for differential verification and live speedup comparisons
- **`crates/rv-web-bridge`**: `wasm-bindgen` bridge compiling the recompiler itself to `wasm32-unknown-unknown`
- **`crates/recomp-cli`**: Native CLI for compilation, Wasmtime execution, differential testing, and benchmarking
- **`guests/`**: Bare-metal C/Rust RISC-V workloads compiled without compressed (`C`) instructions
- **`web/`**: React + TypeScript + Vite interactive demo, live `<canvas>` runner, and 3-pane Compiler Explorer

See [MASTER_PLAN.md](./MASTER_PLAN.md) for the full technical specification and phased roadmap.
