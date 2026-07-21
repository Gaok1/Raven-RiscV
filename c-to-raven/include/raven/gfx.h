#ifndef RAVEN_GFX_H
#define RAVEN_GFX_H

#include <raven/types.h>
#include <raven/_ecall.h>

/* Raven graphics API (syscalls 2000+).
 *
 * The framebuffer lives on the host. Drawing writes to a back buffer;
 * raven_screen_present() publishes it. Colors are 0x00RRGGBB.
 */

#define RAVEN_KEY_NONE      0u
#define RAVEN_KEY_BACKSPACE 8u
#define RAVEN_KEY_TAB       9u
#define RAVEN_KEY_ENTER     13u
#define RAVEN_KEY_ESC       27u

#define RAVEN_KEY_SPACE         ((raven_u32)' ')
#define RAVEN_KEY_EXCLAMATION   ((raven_u32)'!')
#define RAVEN_KEY_DOUBLE_QUOTE  ((raven_u32)'"')
#define RAVEN_KEY_HASH          ((raven_u32)'#')
#define RAVEN_KEY_DOLLAR        ((raven_u32)'$')
#define RAVEN_KEY_PERCENT       ((raven_u32)'%')
#define RAVEN_KEY_AMPERSAND     ((raven_u32)'&')
#define RAVEN_KEY_APOSTROPHE    ((raven_u32)'\'')
#define RAVEN_KEY_LEFT_PAREN    ((raven_u32)'(')
#define RAVEN_KEY_RIGHT_PAREN   ((raven_u32)')')
#define RAVEN_KEY_ASTERISK      ((raven_u32)'*')
#define RAVEN_KEY_PLUS          ((raven_u32)'+')
#define RAVEN_KEY_COMMA         ((raven_u32)',')
#define RAVEN_KEY_MINUS         ((raven_u32)'-')
#define RAVEN_KEY_PERIOD        ((raven_u32)'.')
#define RAVEN_KEY_SLASH         ((raven_u32)'/')
#define RAVEN_KEY_0             ((raven_u32)'0')
#define RAVEN_KEY_1             ((raven_u32)'1')
#define RAVEN_KEY_2             ((raven_u32)'2')
#define RAVEN_KEY_3             ((raven_u32)'3')
#define RAVEN_KEY_4             ((raven_u32)'4')
#define RAVEN_KEY_5             ((raven_u32)'5')
#define RAVEN_KEY_6             ((raven_u32)'6')
#define RAVEN_KEY_7             ((raven_u32)'7')
#define RAVEN_KEY_8             ((raven_u32)'8')
#define RAVEN_KEY_9             ((raven_u32)'9')
#define RAVEN_KEY_COLON         ((raven_u32)':')
#define RAVEN_KEY_SEMICOLON     ((raven_u32)';')
#define RAVEN_KEY_LESS_THAN     ((raven_u32)'<')
#define RAVEN_KEY_EQUALS        ((raven_u32)'=')
#define RAVEN_KEY_GREATER_THAN  ((raven_u32)'>')
#define RAVEN_KEY_QUESTION      ((raven_u32)'?')
#define RAVEN_KEY_AT            ((raven_u32)'@')
#define RAVEN_KEY_A             ((raven_u32)'a')
#define RAVEN_KEY_B             ((raven_u32)'b')
#define RAVEN_KEY_C             ((raven_u32)'c')
#define RAVEN_KEY_D             ((raven_u32)'d')
#define RAVEN_KEY_E             ((raven_u32)'e')
#define RAVEN_KEY_F             ((raven_u32)'f')
#define RAVEN_KEY_G             ((raven_u32)'g')
#define RAVEN_KEY_H             ((raven_u32)'h')
#define RAVEN_KEY_I             ((raven_u32)'i')
#define RAVEN_KEY_J             ((raven_u32)'j')
#define RAVEN_KEY_K             ((raven_u32)'k')
#define RAVEN_KEY_L             ((raven_u32)'l')
#define RAVEN_KEY_M             ((raven_u32)'m')
#define RAVEN_KEY_N             ((raven_u32)'n')
#define RAVEN_KEY_O             ((raven_u32)'o')
#define RAVEN_KEY_P             ((raven_u32)'p')
#define RAVEN_KEY_Q             ((raven_u32)'q')
#define RAVEN_KEY_R             ((raven_u32)'r')
#define RAVEN_KEY_S             ((raven_u32)'s')
#define RAVEN_KEY_T             ((raven_u32)'t')
#define RAVEN_KEY_U             ((raven_u32)'u')
#define RAVEN_KEY_V             ((raven_u32)'v')
#define RAVEN_KEY_W             ((raven_u32)'w')
#define RAVEN_KEY_X             ((raven_u32)'x')
#define RAVEN_KEY_Y             ((raven_u32)'y')
#define RAVEN_KEY_Z             ((raven_u32)'z')
#define RAVEN_KEY_LEFT_BRACKET  ((raven_u32)'[')
#define RAVEN_KEY_BACKSLASH     ((raven_u32)'\\')
#define RAVEN_KEY_RIGHT_BRACKET ((raven_u32)']')
#define RAVEN_KEY_CARET         ((raven_u32)'^')
#define RAVEN_KEY_UNDERSCORE    ((raven_u32)'_')
#define RAVEN_KEY_BACKTICK      ((raven_u32)'`')
#define RAVEN_KEY_LEFT_BRACE    ((raven_u32)'{')
#define RAVEN_KEY_PIPE          ((raven_u32)'|')
#define RAVEN_KEY_RIGHT_BRACE   ((raven_u32)'}')
#define RAVEN_KEY_TILDE         ((raven_u32)'~')

#define RAVEN_KEY_UP        256u
#define RAVEN_KEY_DOWN      257u
#define RAVEN_KEY_LEFT      258u
#define RAVEN_KEY_RIGHT     259u

#define raven_is_ascii_key(key) ((raven_u32)(key) <= 0x7Fu)
#define raven_is_printable_ascii_key(key) \
    ((raven_u32)(key) >= RAVEN_KEY_SPACE && (raven_u32)(key) <= RAVEN_KEY_TILDE)
#define raven_key_to_ascii(key) ((char)((raven_u32)(key) & 0x7Fu))

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

#if defined(__STDC_VERSION__) && __STDC_VERSION__ >= 201112L
/* C11 convenience dispatch: these accept either raw raven_u32/integers
 * (0x00RRGGBB) or RavenColor. The *_color functions remain available for C99
 * and for code that wants explicit names. */
#define raven_screen_clear(color) \
    _Generic((color), RavenColor: raven_screen_clear_color, default: raven_screen_clear)(color)

#define raven_screen_set_pixel(x, y, color) \
    _Generic((color), RavenColor: raven_screen_set_pixel_color, default: raven_screen_set_pixel)( \
        (x), (y), (color))

#define raven_screen_fill_rect(x, y, width, height, color) \
    _Generic((color), RavenColor: raven_screen_fill_rect_color, default: raven_screen_fill_rect)( \
        (x), (y), (width), (height), (color))
#endif

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

/* Poll and return a printable ASCII character, or 0 when the pending key is
 * absent, non-printable, or special (arrows, etc.). Non-printable/special keys
 * are consumed. */
static inline char raven_screen_poll_printable_char(void) {
    raven_u32 key = raven_screen_poll_key();
    return raven_is_printable_ascii_key(key) ? (char)key : '\0';
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
