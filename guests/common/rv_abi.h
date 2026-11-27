#ifndef RV_ABI_H
#define RV_ABI_H

#include <stdint.h>
#include <stddef.h>

#define RV_FB_BASE   ((volatile uint32_t *)0x01000000u)
#define RV_MMIO_BASE ((volatile uint32_t *)0x01200000u)

#define RV_MMIO_KEY_LO      0
#define RV_MMIO_KEY_HI      1
#define RV_MMIO_MOUSE_X     2
#define RV_MMIO_MOUSE_Y     3
#define RV_MMIO_MOUSE_BTN   4
#define RV_MMIO_CLOCK_MS    5
#define RV_MMIO_FB_WIDTH    6
#define RV_MMIO_FB_HEIGHT   7
#define RV_MMIO_FRAME_COUNT 8

#define RV_KEY_W      (1u << 0)
#define RV_KEY_A      (1u << 1)
#define RV_KEY_S      (1u << 2)
#define RV_KEY_D      (1u << 3)
#define RV_KEY_UP     (1u << 4)
#define RV_KEY_DOWN   (1u << 5)
#define RV_KEY_LEFT   (1u << 6)
#define RV_KEY_RIGHT  (1u << 7)
#define RV_KEY_SPACE  (1u << 8)
#define RV_KEY_ENTER  (1u << 9)
#define RV_KEY_ESC    (1u << 10)
#define RV_KEY_SHIFT  (1u << 11)
#define RV_KEY_CTRL   (1u << 12)
#define RV_KEY_R      (1u << 13)
#define RV_KEY_P      (1u << 14)

#define SYS_WRITE         1
#define SYS_EXIT          2
#define SYS_CLOCK_MS      3
#define SYS_PRESENT_FRAME 4
#define SYS_POLL_INPUT    5
#define SYS_SBRK          6

static inline int32_t rv_ecall3(uint32_t id, uint32_t a0, uint32_t a1, uint32_t a2) {
    register uint32_t r_a0 __asm__("a0") = a0;
    register uint32_t r_a1 __asm__("a1") = a1;
    register uint32_t r_a2 __asm__("a2") = a2;
    register uint32_t r_a7 __asm__("a7") = id;
    __asm__ volatile("ecall"
                     : "+r"(r_a0)
                     : "r"(r_a1), "r"(r_a2), "r"(r_a7)
                     : "memory");
    return (int32_t)r_a0;
}

static inline int32_t rv_ecall1(uint32_t id, uint32_t a0) {
    return rv_ecall3(id, a0, 0, 0);
}

static inline int32_t rv_ecall0(uint32_t id) {
    return rv_ecall3(id, 0, 0, 0);
}

static inline int32_t rv_write(int fd, const void *buf, uint32_t len) {
    return rv_ecall3(SYS_WRITE, (uint32_t)fd, (uint32_t)(uintptr_t)buf, len);
}

__attribute__((noreturn)) static inline void rv_exit(int code) {
    rv_ecall1(SYS_EXIT, (uint32_t)code);
    for (;;) {}
}

static inline uint32_t rv_clock_ms(void) {
    return (uint32_t)rv_ecall0(SYS_CLOCK_MS);
}

static inline void rv_present_frame(uint32_t width, uint32_t height) {
    (void)rv_ecall3(SYS_PRESENT_FRAME, width, height, 0);
}

static inline uint32_t rv_poll_input(void) {
    return (uint32_t)rv_ecall0(SYS_POLL_INPUT);
}

static inline void *rv_sbrk(int32_t incr) {
    int32_t res = rv_ecall1(SYS_SBRK, (uint32_t)incr);
    if (res == -1) return (void *)-1;
    return (void *)(uintptr_t)(uint32_t)res;
}

#endif
