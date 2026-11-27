#include "rv_abi.h"
#include <stdint.h>

int printf(const char *fmt, ...);

static int failures = 0;

static void check_u32(const char *name, uint32_t got, uint32_t exp) {
    if (got == exp) {
        printf("[PASS] %s = 0x%x\n", name, got);
    } else {
        printf("[FAIL] %s got 0x%x exp 0x%x\n", name, got, exp);
        failures++;
    }
}

__attribute__((noinline)) static int32_t sdiv(int32_t a, int32_t b) {
    int32_t r;
    __asm__ volatile("div %0, %1, %2" : "=r"(r) : "r"(a), "r"(b));
    return r;
}

__attribute__((noinline)) static uint32_t udiv(uint32_t a, uint32_t b) {
    uint32_t r;
    __asm__ volatile("divu %0, %1, %2" : "=r"(r) : "r"(a), "r"(b));
    return r;
}

__attribute__((noinline)) static int32_t srem(int32_t a, int32_t b) {
    int32_t r;
    __asm__ volatile("rem %0, %1, %2" : "=r"(r) : "r"(a), "r"(b));
    return r;
}

__attribute__((noinline)) static uint32_t urem(uint32_t a, uint32_t b) {
    uint32_t r;
    __asm__ volatile("remu %0, %1, %2" : "=r"(r) : "r"(a), "r"(b));
    return r;
}

__attribute__((noinline)) static uint32_t fmin_bits(uint32_t a, uint32_t b) {
    float fa, fb, fr;
    __asm__ volatile("fmv.w.x %0, %1" : "=f"(fa) : "r"(a));
    __asm__ volatile("fmv.w.x %0, %1" : "=f"(fb) : "r"(b));
    __asm__ volatile("fmin.s %0, %1, %2" : "=f"(fr) : "f"(fa), "f"(fb));
    uint32_t out;
    __asm__ volatile("fmv.x.w %0, %1" : "=r"(out) : "f"(fr));
    return out;
}

__attribute__((noinline)) static uint32_t fmax_bits(uint32_t a, uint32_t b) {
    float fa, fb, fr;
    __asm__ volatile("fmv.w.x %0, %1" : "=f"(fa) : "r"(a));
    __asm__ volatile("fmv.w.x %0, %1" : "=f"(fb) : "r"(b));
    __asm__ volatile("fmax.s %0, %1, %2" : "=f"(fr) : "f"(fa), "f"(fb));
    uint32_t out;
    __asm__ volatile("fmv.x.w %0, %1" : "=r"(out) : "f"(fr));
    return out;
}

__attribute__((noinline)) static int32_t fcvt_ws(uint32_t a) {
    float fa;
    int32_t out;
    __asm__ volatile("fmv.w.x %0, %1" : "=f"(fa) : "r"(a));
    __asm__ volatile("fcvt.w.s %0, %1, rne" : "=r"(out) : "f"(fa));
    return out;
}

__attribute__((noinline)) static uint32_t fmadd_bits(uint32_t a, uint32_t b, uint32_t c) {
    float fa, fb, fc, fr;
    __asm__ volatile("fmv.w.x %0, %1" : "=f"(fa) : "r"(a));
    __asm__ volatile("fmv.w.x %0, %1" : "=f"(fb) : "r"(b));
    __asm__ volatile("fmv.w.x %0, %1" : "=f"(fc) : "r"(c));
    __asm__ volatile("fmadd.s %0, %1, %2, %3" : "=f"(fr) : "f"(fa), "f"(fb), "f"(fc));
    uint32_t out;
    __asm__ volatile("fmv.x.w %0, %1" : "=r"(out) : "f"(fr));
    return out;
}

int main(void) {
    printf("=== RV32IMF Compliance Suite ===\n");

    check_u32("DIV(42, 0)", (uint32_t)sdiv(42, 0), 0xFFFFFFFFu);
    check_u32("DIV(INT_MIN, -1)", (uint32_t)sdiv((int32_t)0x80000000u, -1), 0x80000000u);
    check_u32("DIVU(42, 0)", udiv(42, 0), 0xFFFFFFFFu);
    check_u32("REM(42, 0)", (uint32_t)srem(42, 0), 42u);
    check_u32("REM(INT_MIN, -1)", (uint32_t)srem((int32_t)0x80000000u, -1), 0u);
    check_u32("REMU(42, 0)", urem(42, 0), 42u);

    check_u32("FMIN(-0.0, +0.0)", fmin_bits(0x80000000u, 0x00000000u), 0x80000000u);
    check_u32("FMAX(-0.0, +0.0)", fmax_bits(0x80000000u, 0x00000000u), 0x00000000u);
    check_u32("FMIN(qNaN, 1.5)", fmin_bits(0x7FC01234u, 0x3FC00000u), 0x3FC00000u);
    check_u32("FMIN(qNaN, sNaN)", fmin_bits(0x7FC01234u, 0x7F800001u), 0x7FC00000u);
    check_u32("FCVT.W.S(qNaN)", (uint32_t)fcvt_ws(0x7FC00000u), 0x7FFFFFFFu);
    check_u32("FMADD(1+ulp,1+ulp,-1)", fmadd_bits(0x3F800001u, 0x3F800001u, 0xBF800000u), 0x34800000u);

    printf("Summary: %d failure(s)\n", failures);
    return failures == 0 ? 0 : 1;
}
