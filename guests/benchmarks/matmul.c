#include "rv_abi.h"
#include <stdint.h>

int printf(const char *fmt, ...);

#define N 48

static int32_t ia[N * N];
static int32_t ib[N * N];
static int32_t ic[N * N];
static float fa[N * N];
static float fb[N * N];
static float fc[N * N];

int main(void) {
    uint32_t t0 = rv_clock_ms();

    for (int i = 0; i < N * N; i++) {
        ia[i] = (i % 17) - 8;
        ib[i] = ((i * 3) % 19) - 9;
        fa[i] = (float)ia[i] * 0.25f;
        fb[i] = (float)ib[i] * 0.125f;
    }

    for (int r = 0; r < N; r++) {
        for (int c = 0; c < N; c++) {
            int32_t isum = 0;
            float fsum = 0.0f;
            for (int k = 0; k < N; k++) {
                isum += ia[r * N + k] * ib[k * N + c];
                fsum += fa[r * N + k] * fb[k * N + c];
            }
            ic[r * N + c] = isum;
            fc[r * N + c] = fsum;
        }
    }

    uint32_t ichk = 0;
    uint32_t fchk = 0;
    for (int i = 0; i < N * N; i++) {
        ichk ^= (uint32_t)ic[i] + (uint32_t)i;
        union { float f; uint32_t u; } u = { .f = fc[i] };
        fchk ^= u.u;
    }

    uint32_t dt = rv_clock_ms() - t0;
    printf("MatMul %dx%d (RV32IMF): %u ms, ichk=0x%08x fchk=0x%08x\n", N, N, dt, ichk, fchk);
    return 0;
}
