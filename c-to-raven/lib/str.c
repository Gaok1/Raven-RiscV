#include <raven/str.h>

raven_size_t raven_strlen_c(const char *s) {
    raven_size_t n = 0;
    while (*s++) n++;
    return n;
}

int raven_strcmp_c(const char *a, const char *b) {
    while (*a && *a == *b) { a++; b++; }
    return (int)(raven_u8)*a - (int)(raven_u8)*b;
}

int raven_strncmp(const char *a, const char *b, raven_size_t n) {
    while (n && *a && *a == *b) { a++; b++; n--; }
    if (n == 0) return 0;
    return (int)(raven_u8)*a - (int)(raven_u8)*b;
}

char *raven_strcpy(char *dst, const char *src) {
    char *d = dst;
    while ((*d++ = *src++)) { }
    return dst;
}

char *raven_strncpy(char *dst, const char *src, raven_size_t n) {
    char *d = dst;
    while (n && (*d++ = *src++)) n--;
    while (n--) *d++ = '\0';
    return dst;
}

char *raven_strcat(char *dst, const char *src) {
    char *d = dst;
    while (*d) d++;
    while ((*d++ = *src++)) { }
    return dst;
}

char *raven_strchr(const char *s, int c) {
    while (*s) { if (*s == (char)c) return (char *)s; s++; }
    return (c == '\0') ? (char *)s : (char *)0;
}

char *raven_strrchr(const char *s, int c) {
    const char *last = (const char *)0;
    do { if (*s == (char)c) last = s; } while (*s++);
    return (char *)last;
}
