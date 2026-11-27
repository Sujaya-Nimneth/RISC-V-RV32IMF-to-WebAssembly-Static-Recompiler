#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LLVM_BIN="${LLVM_BIN:-/opt/homebrew/opt/llvm/bin}"
LLD_BIN="${LLD_BIN:-/opt/homebrew/opt/lld/bin/ld.lld}"
CLANG="$LLVM_BIN/clang"
OBJDUMP="$LLVM_BIN/llvm-objdump"

if [[ ! -x "$CLANG" ]]; then
  echo "error: clang not found at $CLANG (install Homebrew llvm)" >&2
  exit 1
fi
if [[ ! -x "$LLD_BIN" ]]; then
  LLD_BIN="lld"
fi

OUT_DIR="$ROOT_DIR/web/public/presets"
mkdir -p "$OUT_DIR"

compile_c() {
  local march="$1"
  local mabi="$2"
  local out="$3"
  shift 3
  "$CLANG" \
    --target=riscv32-unknown-elf \
    -march="$march" \
    -mabi="$mabi" \
    -mno-relax \
    -ffreestanding \
    -nostdlib \
    -O2 \
    -fuse-ld="$LLD_BIN" \
    -Wl,-T,"$ROOT_DIR/guests/common/link.ld" \
    -I"$ROOT_DIR/guests/common" \
    "$ROOT_DIR/guests/common/crt0.S" \
    "$ROOT_DIR/guests/common/minilibc.c" \
    "$@" \
    -o "$out"

  # Verify no 16-bit compressed instructions were emitted
  if "$OBJDUMP" -d "$out" | grep -E '^\s*[0-9a-f]+:\s+[0-9a-f]{4}\s' >/dev/null; then
    echo "error: compressed 16-bit instructions detected in $out" >&2
    exit 1
  fi
  echo "Built $out ($march)"
}

if [[ "${1:-}" == "--smoke" ]]; then
  compile_c "rv32im" "ilp32" "$ROOT_DIR/guests/common/hello.elf" "$ROOT_DIR/guests/common/hello.c"
  exit 0
fi

compile_c "rv32im"  "ilp32"  "$ROOT_DIR/guests/common/hello.elf"       "$ROOT_DIR/guests/common/hello.c"
compile_c "rv32imf" "ilp32f" "$OUT_DIR/compliance.elf"                 "$ROOT_DIR/guests/compliance/main.c"
compile_c "rv32imf" "ilp32f" "$OUT_DIR/mandelbrot.elf"                 "$ROOT_DIR/guests/benchmarks/mandelbrot.c"
compile_c "rv32imf" "ilp32f" "$OUT_DIR/matmul.elf"                     "$ROOT_DIR/guests/benchmarks/matmul.c"
compile_c "rv32im"  "ilp32"  "$OUT_DIR/dhrystone.elf"                  "$ROOT_DIR/guests/benchmarks/dhrystone.c"
compile_c "rv32imf" "ilp32f" "$OUT_DIR/raycaster.elf"                  "$ROOT_DIR/guests/raycaster/main.c"
compile_c "rv32im"  "ilp32"  "$OUT_DIR/life.elf"                       "$ROOT_DIR/guests/life/main.c"
