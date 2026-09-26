/* Linker script for riscv-tests on RAVEN.
   The official environment links at 0x80000000; RAVEN's RAM starts at 0 and
   is 16 MiB by default, so the program is linked at 0x10000. */
OUTPUT_ARCH( "riscv" )
ENTRY( _start )

SECTIONS
{
  . = 0x00010000;
  .text.init : { *(.text.init) }
  . = ALIGN(0x1000);
  .text : { *(.text) }
  . = ALIGN(0x1000);
  .data : { *(.data) }
  .bss : { *(.bss) }
  _end = .;
}
