// main.c — guessing game
//
// Demonstrates:
//   - rand_range()           random number in [lo, hi)
//   - read_int()             read a signed integer from stdin
//   - print_int/str/ln()     basic output helpers
//   - print_bin()            32-bit binary display (try it with the secret number!)
//   - eprint_str()           stderr debug messages
//   - raven_pause()          freeze execution to inspect state in Raven
//
// Run in Raven, open the Console tab, and play!

#include "raven.h"

int main(void) {
    print_str("=== Guess the number! ===\n");
    print_str("I picked a number between 1 and 100.\n\n");

    unsigned int secret = rand_range(1, 101);   // [1, 100]
    eprint_str("[debug] secret = ");             // visible on stderr (red in console)
    eprint_int((int)secret);
    eprint_ln();

    int attempts = 0;

    while (1) {
        print_str("Your guess: ");
        int guess = read_int();
        attempts++;

        if (guess < 1 || guess > 100) {
            print_str("  Out of range! Try between 1 and 100.\n");
            continue;
        }

        if ((unsigned int)guess < secret) {
            print_str("  Too low!\n");
        } else if ((unsigned int)guess > secret) {
            print_str("  Too high!\n");
        } else {
            print_str("\nCorrect! You got it in ");
            print_int(attempts);
            print_str(attempts == 1 ? " attempt.\n" : " attempts.\n");

            print_str("The number ");
            print_uint(secret);
            print_str(" in binary: ");
            print_bin(secret);
            print_ln();

            break;
        }
    }

    raven_pause(); // inspect registers and memory before exit
    return 0;
}
