#include <raven/raven.h>

#define N 5000

static int arr[N];

int main(void) {
    for (int i = 0; i < N; i++) arr[i] = N - 1 - i;

    RAVEN_MEASURE("bubble sort", {
        for (int i = 0; i < N - 1; i++) {
            for (int j = 0; j < N - 1 - i; j++) {
                if (arr[j] > arr[j + 1]) {
                    int tmp = arr[j];
                    arr[j] = arr[j + 1];
                    arr[j + 1] = tmp;
                }
            }
        }
    });

    int sum = 0;
    for (int i = 0; i < N; i++) sum += arr[i];

    raven_print_str("sum = ");
    raven_print_int(sum);
    raven_println();
    return 0;
}
