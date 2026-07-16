#ifndef RAVEN_GFX_H
#define RAVEN_GFX_H

#include <raven/types.h>
#include <raven/_ecall.h>

/* Raven graphics API (syscalls 2000+).
 *
 * The framebuffer lives on the host. Drawing writes to a back buffer;
 * raven_screen_present() publishes it. Colors are 0x00RRGGBB.
 */

#define RAVEN_KEY_ENTER     13u
#define RAVEN_KEY_BACKSPACE 8u
#define RAVEN_KEY_ESC       27u
#define RAVEN_KEY_UP        256u
#define RAVEN_KEY_DOWN      257u
#define RAVEN_KEY_LEFT      258u
#define RAVEN_KEY_RIGHT     259u

#define RAVEN_RGB(r, g, b) \
    ((((raven_u32)(r) & 255u) << 16) | (((raven_u32)(g) & 255u) << 8) | ((raven_u32)(b) & 255u))

typedef struct RavenColor {
    raven_u8 r;
    raven_u8 g;
    raven_u8 b;
} RavenColor;

static inline RavenColor raven_color_rgb(raven_u8 r, raven_u8 g, raven_u8 b) {
    RavenColor color = { r, g, b };
    return color;
}

static inline RavenColor raven_color_from_bytes(const raven_u8 bytes[3]) {
    RavenColor color = { bytes[0], bytes[1], bytes[2] };
    return color;
}

static inline raven_u32 raven_color_pack(RavenColor color) {
    return RAVEN_RGB(color.r, color.g, color.b);
}

static inline int raven_screen_init(raven_u32 width, raven_u32 height) {
    register int       _a7 __asm__("a7") = RAVEN_ECALL_SCREEN_INIT;
    register raven_u32 _a0 __asm__("a0") = width;
    register raven_u32 _a1 __asm__("a1") = height;
    __asm__ volatile("ecall" : "+r"(_a0) : "r"(_a7), "r"(_a1) : "memory");
    return (int)_a0;
}

static inline int raven_screen_clear(raven_u32 rgb) {
    register int       _a7 __asm__("a7") = RAVEN_ECALL_SCREEN_CLEAR;
    register raven_u32 _a0 __asm__("a0") = rgb;
    __asm__ volatile("ecall" : "+r"(_a0) : "r"(_a7) : "memory");
    return (int)_a0;
}

static inline int raven_screen_clear_color(RavenColor color) {
    return raven_screen_clear(raven_color_pack(color));
}

static inline int raven_screen_set_pixel(raven_u32 x, raven_u32 y, raven_u32 rgb) {
    register int       _a7 __asm__("a7") = RAVEN_ECALL_SCREEN_SET_PIXEL;
    register raven_u32 _a0 __asm__("a0") = x;
    register raven_u32 _a1 __asm__("a1") = y;
    register raven_u32 _a2 __asm__("a2") = rgb;
    __asm__ volatile("ecall" : "+r"(_a0) : "r"(_a7), "r"(_a1), "r"(_a2) : "memory");
    return (int)_a0;
}

static inline int raven_screen_set_pixel_color(raven_u32 x, raven_u32 y, RavenColor color) {
    return raven_screen_set_pixel(x, y, raven_color_pack(color));
}

static inline int raven_screen_fill_rect(raven_u32 x, raven_u32 y,
                                         raven_u32 width, raven_u32 height,
                                         raven_u32 rgb) {
    register int       _a7 __asm__("a7") = RAVEN_ECALL_SCREEN_FILL_RECT;
    register raven_u32 _a0 __asm__("a0") = x;
    register raven_u32 _a1 __asm__("a1") = y;
    register raven_u32 _a2 __asm__("a2") = width;
    register raven_u32 _a3 __asm__("a3") = height;
    register raven_u32 _a4 __asm__("a4") = rgb;
    __asm__ volatile("ecall" : "+r"(_a0)
                              : "r"(_a7), "r"(_a1), "r"(_a2), "r"(_a3), "r"(_a4)
                              : "memory");
    return (int)_a0;
}

static inline int raven_screen_fill_rect_color(raven_u32 x, raven_u32 y,
                                               raven_u32 width, raven_u32 height,
                                               RavenColor color) {
    return raven_screen_fill_rect(x, y, width, height, raven_color_pack(color));
}

static inline int raven_screen_present(void) {
    register int _a7 __asm__("a7") = RAVEN_ECALL_SCREEN_PRESENT;
    register int _a0 __asm__("a0");
    __asm__ volatile("ecall" : "=r"(_a0) : "r"(_a7) : "memory");
    return _a0;
}

/* Non-blocking. Returns 0 when no key is pending; printable keys are lowercase
 * ASCII, arrows are RAVEN_KEY_UP/DOWN/LEFT/RIGHT. */
static inline raven_u32 raven_screen_poll_key(void) {
    register int       _a7 __asm__("a7") = RAVEN_ECALL_SCREEN_POLL_KEY;
    register raven_u32 _a0 __asm__("a0");
    __asm__ volatile("ecall" : "=r"(_a0) : "r"(_a7) : "memory");
    return _a0;
}

/* Wall-clock milliseconds since raven_screen_init(). */
static inline raven_u32 raven_screen_time_ms(void) {
    register int       _a7 __asm__("a7") = RAVEN_ECALL_SCREEN_TIME_MS;
    register raven_u32 _a0 __asm__("a0");
    __asm__ volatile("ecall" : "=r"(_a0) : "r"(_a7));
    return _a0;
}

/* Frame pacing. Parks the current hart until the wall-clock deadline. */
static inline int raven_screen_sleep_ms(raven_u32 ms) {
    register int       _a7 __asm__("a7") = RAVEN_ECALL_SCREEN_SLEEP_MS;
    register raven_u32 _a0 __asm__("a0") = ms;
    __asm__ volatile("ecall" : "+r"(_a0) : "r"(_a7) : "memory");
    return (int)_a0;
}

#endif
