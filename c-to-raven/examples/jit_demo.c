/* JIT demo — generate RISC-V machine code at runtime, mark it executable,
 * and jump into it. Uses <raven/advanced.h> for raven_unsafe_map_exec. */

#include <raven/raven.h>
#include <raven/advanced.h>

int main(void) {
    /* Two-instruction function that returns a0 + a1:
     *   add a0, a0, a1      -> 0x00b50533
     *   jr  ra              -> 0x00008067
     */
    static int code[] = {
        0x00b50533,
        0x00008067,
    };

    raven_unsafe_map_exec(code, sizeof(code));

    int (*sum)(int, int) = (int (*)(int, int))code;

    raven_print_str("20 + 13 = ");
    raven_print_int(sum(20, 13));
    raven_println();
    return 0;
}
