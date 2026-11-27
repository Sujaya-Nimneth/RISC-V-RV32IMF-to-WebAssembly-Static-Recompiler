#include <stdint.h>

int printf(const char *fmt, ...);

int main(void) {
    int sum = 0;
    for (int i = 1; i <= 10; i++) {
        sum += i;
    }
    printf("Hello from RV32IM! sum(1..10) = %d\n", sum);
    return 0;
}
