#include "raven.h"
#include <stdint.h>

RAVEN_HART_STACK(worker_stack_1, 1024);
RAVEN_HART_STACK(worker_stack_2, 1024);

enum {
    ARRAY_LEN = 20,
    SORTING_OFFSET = 10,
};

typedef struct {
    int *arr;
    unsigned int left;
    unsigned int right;
} SortingArgs;

static int *random_arr(int len)
{
    int *arr = malloc((size_t)len * sizeof(int));
    
    for (int i = 0; i < len; i++) {
        arr[i] = (int)(rand_i32() % 100);
    }

    return arr;
}

static void print_array(const int *arr, unsigned int len)
{
    for (unsigned int i = 0; i < len; i++) {
        raven_print_int(arr[i]);
        if (i + 1 < len) {
            raven_print_str(", ");
        }
    }
    raven_print_newline();
}

static void sort_range(int *arr, unsigned int left, unsigned int right)
{
    for (unsigned int i = left; i < right; i++) {
        unsigned int min_index = i;

        for (unsigned int j = i + 1; j < right; j++) {
            if (arr[j] < arr[min_index]) {
                min_index = j;
            }
        }

        if (min_index != i) {
            int temp = arr[i];
            arr[i] = arr[min_index];
            arr[min_index] = temp;
        }
    }
}

static void sort_worker(unsigned int raw_arg)
{
    SortingArgs *args = (SortingArgs *)(uintptr_t)raw_arg;
    sort_range(args->arr, args->left, args->right);
}

int main(void)
{
    int *arr = random_arr(ARRAY_LEN);
    SortingArgs first = { arr, 0, SORTING_OFFSET };
    SortingArgs second = { arr, SORTING_OFFSET, ARRAY_LEN };

    raven_println_str("Before:");
    print_array(arr, ARRAY_LEN);

    RavenHartHandle h1 =
        raven_spawn_hart_array(sort_worker, worker_stack_1, (unsigned int)(uintptr_t)&first);
    RavenHartHandle h2 =
        raven_spawn_hart_array(sort_worker, worker_stack_2, (unsigned int)(uintptr_t)&second);

    h1.join(&h1);
    h2.join(&h2);

    raven_println_str("After sorting each half:");
    print_array(arr, ARRAY_LEN);

    free(arr);
    return 0;
}
