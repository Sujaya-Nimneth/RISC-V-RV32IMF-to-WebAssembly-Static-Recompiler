#include "rv_abi.h"
#include <stdint.h>

int printf(const char *fmt, ...);
char *strcpy(char *dst, const char *src);
int strcmp(const char *a, const char *b);
void *malloc(size_t size);

typedef struct record {
    struct record *ptr_comp;
    int discr;
    int enum_comp;
    int int_comp;
    char str_comp[32];
} rec_t;

static rec_t *ptr_glob;
static rec_t *next_ptr_glob;
static int int_glob;
static int arr1_glob[50];
static int arr2_glob[50][50];

static void proc6(int enum_val, int *enum_out) {
    *enum_out = enum_val;
    switch (enum_val) {
        case 0: *enum_out = 0; break;
        case 1: *enum_out = (int_glob > 100) ? 0 : 3; break;
        case 2: *enum_out = 1; break;
        case 3: break;
        case 4: *enum_out = 2; break;
    }
}

static void proc7(int p1, int p2, int *out) {
    int loc = p1 + 2;
    *out = p2 + loc;
}

static void proc8(int arr1[50], int arr2[50][50], int p1, int p2) {
    int loc = p1 + 5;
    arr1[loc] = p2;
    arr1[loc + 1] = arr1[loc];
    arr1[loc + 30] = loc;
    for (int i = loc; i <= loc + 1; i++) {
        arr2[loc][i] = loc;
    }
    arr2[loc][loc - 1] += 1;
    arr2[loc + 20][loc] = arr1[loc];
    int_glob = 5;
}

static int func1(char c1, char c2) {
    char ch1 = c1;
    char ch2 = c2;
    if (ch2 != ch1) return 0;
    return 1;
}

static int func2(const char *s1, const char *s2) {
    int loc = 2;
    char ch = 'A';
    while (loc <= 2) {
        if (func1(s1[loc], s2[loc + 1]) == 0) {
            ch = 'A';
            loc += 1;
        }
    }
    if (ch >= 'W' && ch < 'Z') loc = 7;
    if (strcmp(s1, s2) > 0) {
        loc += 7;
        return 1;
    }
    return 0;
}

int main(void) {
    uint32_t t0 = rv_clock_ms();
    ptr_glob = (rec_t *)malloc(sizeof(rec_t));
    next_ptr_glob = (rec_t *)malloc(sizeof(rec_t));
    ptr_glob->ptr_comp = next_ptr_glob;
    ptr_glob->discr = 0;
    ptr_glob->enum_comp = 2;
    ptr_glob->int_comp = 40;
    strcpy(ptr_glob->str_comp, "DHRYSTONE PROGRAM, SOME STRING");

    int runs = 50000;
    uint32_t acc = 0;
    for (int i = 0; i < runs; i++) {
        int out_int = 0;
        int out_enum = 0;
        char s1[32];
        char s2[32];
        strcpy(s1, "DHRYSTONE PROGRAM, 1'ST STRING");
        strcpy(s2, "DHRYSTONE PROGRAM, 2'ND STRING");
        int_glob = func2(s1, s2) ? 1 : 0;
        proc7(2, 3, &out_int);
        proc6(i % 5, &out_enum);
        proc8(arr1_glob, arr2_glob, 5, out_int + out_enum);
        acc = (acc * 31u) ^ (uint32_t)(arr2_glob[10][10] + out_int + out_enum + int_glob);
    }

    uint32_t dt = rv_clock_ms() - t0;
    printf("Dhrystone (%d iters): %u ms, acc=0x%08x\n", runs, dt, acc);
    return 0;
}
