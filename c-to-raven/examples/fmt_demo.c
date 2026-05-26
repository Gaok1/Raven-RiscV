#include <raven/raven.h>

/* Smoke test for <raven/fmt.h>. Exercises printf, snprintf and sscanf
 * across every supported conversion. raven_scanf (stdin) is omitted —
 * non-interactive runs would block. For float formatting use
 * raven_print_float_n from <raven/io.h>. */

int main(void) {
    raven_printf("── raven_printf ──\n");

    raven_printf("int:    %d  %+d  % d  %5d  %-5d|  %05d\n",
                 -42, 42, 42, 42, 42, 42);
    raven_printf("uint:   %u  %10u\n", 4000000000u, 12u);
    raven_printf("hex:    %x  %X  %#x  %08x\n", 0xDEADu, 0xBEEFu, 0xCAFEu, 0x42u);
    raven_printf("oct/b:  %o  %#o  %b\n", 0755u, 0755u, 0x16u);
    raven_printf("char:   '%c'   pct:   '%%'\n", 'Q');
    raven_printf("str:    [%s]  [%10s]  [%-10s]  [%.3s]\n",
                 "raven", "raven", "raven", "raven");
    raven_printf("ptr:    %p\n", (void *)0x1234);

    raven_printf("\n── raven_snprintf ──\n");
    char buf[64];
    int n = raven_snprintf(buf, sizeof(buf),
                           "%s=%d (0x%X)", "answer", 42, 42);
    raven_printf("wrote %d bytes: [%s]\n", n, buf);

    /* truncation: returns full length, buffer is NUL-terminated */
    char small[8];
    int  m = raven_snprintf(small, sizeof(small),
                            "abcdefghijklmno");
    raven_printf("truncate: ret=%d  buf=[%s]\n", m, small);

    raven_printf("\n── raven_sscanf ──\n");
    int a = 0, b = 0;
    raven_u32 hex = 0;
    char word[16];
    int got = raven_sscanf("  -17  0xFF   hello  42",
                           "%d %x %s %d", &a, &hex, word, &b);
    raven_printf("matched %d: a=%d hex=0x%X word=[%s] b=%d\n",
                 got, a, hex, word, b);

    return 0;
}
