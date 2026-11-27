#include "rv_abi.h"
#include <stdarg.h>
#include <stdint.h>
#include <stddef.h>

void *memcpy(void *dst, const void *src, size_t n) {
    uint8_t *d = (uint8_t *)dst;
    const uint8_t *s = (const uint8_t *)src;
    for (size_t i = 0; i < n; i++) d[i] = s[i];
    return dst;
}

void *memmove(void *dst, const void *src, size_t n) {
    uint8_t *d = (uint8_t *)dst;
    const uint8_t *s = (const uint8_t *)src;
    if (d < s) {
        for (size_t i = 0; i < n; i++) d[i] = s[i];
    } else if (d > s) {
        for (size_t i = n; i > 0; i--) d[i - 1] = s[i - 1];
    }
    return dst;
}

void *memset(void *dst, int c, size_t n) {
    uint8_t *d = (uint8_t *)dst;
    for (size_t i = 0; i < n; i++) d[i] = (uint8_t)c;
    return dst;
}

int memcmp(const void *a, const void *b, size_t n) {
    const uint8_t *pa = (const uint8_t *)a;
    const uint8_t *pb = (const uint8_t *)b;
    for (size_t i = 0; i < n; i++) {
        if (pa[i] != pb[i]) return (int)pa[i] - (int)pb[i];
    }
    return 0;
}

size_t strlen(const char *s) {
    size_t n = 0;
    while (s[n]) n++;
    return n;
}

int strcmp(const char *a, const char *b) {
    while (*a && (*a == *b)) { a++; b++; }
    return (int)(uint8_t)*a - (int)(uint8_t)*b;
}

int strncmp(const char *a, const char *b, size_t n) {
    for (size_t i = 0; i < n; i++) {
        if (a[i] != b[i] || a[i] == 0) return (int)(uint8_t)a[i] - (int)(uint8_t)b[i];
    }
    return 0;
}

char *strcpy(char *dst, const char *src) {
    char *d = dst;
    while ((*d++ = *src++) != 0) {}
    return dst;
}

char *strncpy(char *dst, const char *src, size_t n) {
    size_t i = 0;
    for (; i < n && src[i]; i++) dst[i] = src[i];
    for (; i < n; i++) dst[i] = 0;
    return dst;
}

typedef struct {
    size_t size;
} alloc_hdr_t;

void *malloc(size_t size) {
    if (size == 0) size = 1;
    size_t aligned = (size + 15u) & ~15u;
    size_t total = aligned + 16u;
    void *p = rv_sbrk((int32_t)total);
    if (p == (void *)-1) return NULL;
    alloc_hdr_t *hdr = (alloc_hdr_t *)p;
    hdr->size = aligned;
    return (void *)((uint8_t *)p + 16);
}

void free(void *ptr) {
    (void)ptr;
}

void *calloc(size_t nmemb, size_t size) {
    size_t total = nmemb * size;
    void *p = malloc(total);
    if (p) memset(p, 0, total);
    return p;
}

void *realloc(void *ptr, size_t new_size) {
    if (!ptr) return malloc(new_size);
    alloc_hdr_t *hdr = (alloc_hdr_t *)((uint8_t *)ptr - 16);
    if (new_size <= hdr->size) return ptr;
    void *np = malloc(new_size);
    if (np) memcpy(np, ptr, hdr->size);
    return np;
}

int putchar(int c) {
    char ch = (char)c;
    rv_write(1, &ch, 1);
    return c;
}

int puts(const char *s) {
    size_t len = strlen(s);
    if (len > 0) rv_write(1, s, (uint32_t)len);
    rv_write(1, "\n", 1);
    return 0;
}

static void fmt_uint(char *buf, size_t cap, size_t *pos, uint32_t val, uint32_t base, int upper) {
    const char *digits = upper ? "0123456789ABCDEF" : "0123456789abcdef";
    char tmp[32];
    int n = 0;
    if (val == 0) tmp[n++] = '0';
    while (val > 0) {
        tmp[n++] = digits[val % base];
        val /= base;
    }
    for (int i = n - 1; i >= 0; i--) {
        if (*pos + 1 < cap) buf[*pos] = tmp[i];
        (*pos)++;
    }
}

int vsnprintf(char *buf, size_t cap, const char *fmt, va_list ap) {
    size_t pos = 0;
    for (; *fmt; fmt++) {
        if (*fmt != '%') {
            if (pos + 1 < cap) buf[pos] = *fmt;
            pos++;
            continue;
        }
        fmt++;
        if (*fmt == 'l') fmt++;
        switch (*fmt) {
            case '%':
                if (pos + 1 < cap) buf[pos] = '%';
                pos++;
                break;
            case 'c': {
                char c = (char)va_arg(ap, int);
                if (pos + 1 < cap) buf[pos] = c;
                pos++;
                break;
            }
            case 's': {
                const char *s = va_arg(ap, const char *);
                if (!s) s = "(null)";
                while (*s) {
                    if (pos + 1 < cap) buf[pos] = *s;
                    pos++;
                    s++;
                }
                break;
            }
            case 'd':
            case 'i': {
                int32_t v = va_arg(ap, int32_t);
                uint32_t uv;
                if (v < 0) {
                    if (pos + 1 < cap) buf[pos] = '-';
                    pos++;
                    uv = (uint32_t)(-(int64_t)v);
                } else {
                    uv = (uint32_t)v;
                }
                fmt_uint(buf, cap, &pos, uv, 10, 0);
                break;
            }
            case 'u':
                fmt_uint(buf, cap, &pos, va_arg(ap, uint32_t), 10, 0);
                break;
            case 'x':
                fmt_uint(buf, cap, &pos, va_arg(ap, uint32_t), 16, 0);
                break;
            case 'X':
                fmt_uint(buf, cap, &pos, va_arg(ap, uint32_t), 16, 1);
                break;
            default:
                if (pos + 1 < cap) buf[pos] = *fmt;
                pos++;
                break;
        }
    }
    if (cap > 0) {
        buf[pos < cap ? pos : cap - 1] = '\0';
    }
    return (int)pos;
}

int snprintf(char *buf, size_t cap, const char *fmt, ...) {
    va_list ap;
    va_start(ap, fmt);
    int r = vsnprintf(buf, cap, fmt, ap);
    va_end(ap);
    return r;
}

int printf(const char *fmt, ...) {
    char buf[512];
    va_list ap;
    va_start(ap, fmt);
    int n = vsnprintf(buf, sizeof(buf), fmt, ap);
    va_end(ap);
    if (n > 0) {
        uint32_t out_len = (uint32_t)(n < (int)sizeof(buf) ? n : (int)sizeof(buf) - 1);
        rv_write(1, buf, out_len);
    }
    return n;
}
