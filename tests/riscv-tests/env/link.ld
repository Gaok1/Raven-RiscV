/* Linker script for riscv-tests on RAVEN, for either environment.
   The official one links at 0x80000000; RAVEN's RAM starts at 0 and is
   16 MiB by default, so the program is linked at 0x10000. Otherwise it is
   the suite's env/p/link.ld: env/p puts `tohost` and `fromhost` in a page
   of their own, and RAVEN finds `tohost` through the symbol table. */
OUTPUT_ARCH( "riscv" )
ENTRY( _start )

SECTIONS
{
  . = 0x00010000;
  .text.init : { *(.text.init) }
  . = ALIGN(0x1000);
  .tohost : { *(.tohost) }
  . = ALIGN(0x1000);
  .text : { *(.text) }
  . = ALIGN(0x1000);
  .data : { *(.data) }
  .bss : { *(.bss) }
  _end = .;
}
