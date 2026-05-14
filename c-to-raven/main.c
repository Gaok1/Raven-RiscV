#include "raven.h"
#include <stdint.h>


int main(void)
{
    int instructions[] = {// Just in time compilation
       0x00b50533, // add a0, a0,a1
       0x00008067 // jr ra
    };

    
    int (*soma)(int, int) = (  int(*)(int, int)    )instructions;
    
    
    __sys_raven_map_exec(instructions, sizeof(instructions));
    
    raven_print_str("a soma de 20 + 13 = \n");
    
    raven_print_int( soma(20, 13)); 
    
    return 0;
}
