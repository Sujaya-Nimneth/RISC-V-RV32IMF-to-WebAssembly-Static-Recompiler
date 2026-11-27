#include "rv_abi.h"
#include <stdint.h>

int printf(const char *fmt, ...);

#define W 320
#define H 200
#define GW 160
#define GH 100

static uint8_t grid_a[GW * GH];
static uint8_t grid_b[GW * GH];

static uint32_t rng_state = 0x12345678u;
static uint32_t xorshift32(void) {
    uint32_t x = rng_state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    rng_state = x;
    return x;
}

static void seed_gosper_and_soup(void) {
    for (int i = 0; i < GW * GH; i++) grid_a[i] = 0;

    // Gosper glider gun
    static const int gun[][2] = {
        {1,5},{1,6},{2,5},{2,6},
        {11,5},{11,6},{11,7},{12,4},{12,8},{13,3},{13,9},{14,3},{14,9},
        {15,6},{16,4},{16,8},{17,5},{17,6},{17,7},{18,6},
        {21,3},{21,4},{21,5},{22,3},{22,4},{22,5},{23,2},{23,6},
        {25,1},{25,2},{25,6},{25,7},
        {35,3},{35,4},{36,3},{36,4}
    };
    for (int i = 0; i < (int)(sizeof(gun) / sizeof(gun[0])); i++) {
        grid_a[(gun[i][1] + 10) * GW + (gun[i][0] + 10)] = 1;
    }

    // Random soup in lower-right
    for (int y = 45; y < 85; y++) {
        for (int x = 80; x < 140; x++) {
            grid_a[y * GW + x] = (xorshift32() & 3u) == 0 ? 1 : 0;
        }
    }
}

int main(void) {
    volatile uint32_t *fb = RV_FB_BASE;
    seed_gosper_and_soup();
    int paused = 0;
    uint32_t prev_keys = 0;

    printf("Conway's Game of Life (Space=Pause, R=Reseed, Mouse=Paint)\n");

    for (;;) {
        uint32_t keys = rv_poll_input();
        if ((keys & RV_KEY_SPACE) && !(prev_keys & RV_KEY_SPACE)) paused = !paused;
        if ((keys & RV_KEY_R) && !(prev_keys & RV_KEY_R)) seed_gosper_and_soup();
        prev_keys = keys;

        if (RV_MMIO_BASE[RV_MMIO_MOUSE_BTN] & 1u) {
            int mx = (int)RV_MMIO_BASE[RV_MMIO_MOUSE_X] / 2;
            int my = (int)RV_MMIO_BASE[RV_MMIO_MOUSE_Y] / 2;
            for (int dy = -1; dy <= 1; dy++) {
                for (int dx = -1; dx <= 1; dx++) {
                    int gx = (mx + dx + GW) % GW;
                    int gy = (my + dy + GH) % GH;
                    grid_a[gy * GW + gx] = 1;
                }
            }
        }

        if (!paused) {
            for (int y = 0; y < GH; y++) {
                int ym = (y == 0) ? (GH - 1) : (y - 1);
                int yp = (y + 1 == GH) ? 0 : (y + 1);
                for (int x = 0; x < GW; x++) {
                    int xm = (x == 0) ? (GW - 1) : (x - 1);
                    int xp = (x + 1 == GW) ? 0 : (x + 1);
                    int n = grid_a[ym * GW + xm] + grid_a[ym * GW + x] + grid_a[ym * GW + xp]
                          + grid_a[y * GW + xm] + grid_a[y * GW + xp]
                          + grid_a[yp * GW + xm] + grid_a[yp * GW + x] + grid_a[yp * GW + xp];
                    uint8_t alive = grid_a[y * GW + x];
                    grid_b[y * GW + x] = (n == 3 || (alive && n == 2)) ? 1 : 0;
                }
            }
            for (int i = 0; i < GW * GH; i++) grid_a[i] = grid_b[i];
        }

        for (int y = 0; y < GH; y++) {
            for (int x = 0; x < GW; x++) {
                uint32_t col = grid_a[y * GW + x] ? 0xFF6CF0A0u : 0xFF14100Cu;
                int py = y * 2, px = x * 2;
                fb[py * W + px] = col;
                fb[py * W + px + 1] = col;
                fb[(py + 1) * W + px] = col;
                fb[(py + 1) * W + px + 1] = col;
            }
        }

        rv_present_frame(W, H);
    }
    return 0;
}
