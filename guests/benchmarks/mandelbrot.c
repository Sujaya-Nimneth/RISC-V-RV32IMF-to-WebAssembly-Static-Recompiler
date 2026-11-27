#include "rv_abi.h"
#include <stdint.h>

int printf(const char *fmt, ...);

#define W 320
#define H 200
#define MAX_ITER 48

static uint32_t pack_rgba(uint8_t r, uint8_t g, uint8_t b) {
    return (uint32_t)r | ((uint32_t)g << 8) | ((uint32_t)b << 16) | 0xFF000000u;
}

int main(void) {
    uint32_t t0 = rv_clock_ms();
    volatile uint32_t *fb = RV_FB_BASE;
    uint32_t checksum = 0;

    for (int py = 0; py < H; py++) {
        float y0 = ((float)py / (float)H) * 2.4f - 1.2f;
        for (int px = 0; px < W; px++) {
            float x0 = ((float)px / (float)W) * 3.5f - 2.5f;
            float x = 0.0f;
            float y = 0.0f;
            int iter = 0;
            while (x * x + y * y <= 4.0f && iter < MAX_ITER) {
                float xtemp = x * x - y * y + x0;
                y = 2.0f * x * y + y0;
                x = xtemp;
                iter++;
            }
            uint32_t col;
            if (iter == MAX_ITER) {
                col = pack_rgba(10, 12, 20);
            } else {
                uint8_t r = (uint8_t)((iter * 13) & 0xFF);
                uint8_t g = (uint8_t)((iter * 7 + 40) & 0xFF);
                uint8_t b = (uint8_t)((255 - iter * 4) & 0xFF);
                col = pack_rgba(r, g, b);
            }
            fb[py * W + px] = col;
            checksum = (checksum * 16777619u) ^ col;
        }
    }

    rv_present_frame(W, H);
    uint32_t dt = rv_clock_ms() - t0;
    printf("Mandelbrot 320x200 (RV32IMF): %u ms, checksum=0x%08x\n", dt, checksum);
    return 0;
}
