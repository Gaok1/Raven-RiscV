/* Linker script for the ELF loader fixture (fixture.S).
   lld, linking rust-to-raven without a script, puts the read-only data at the
   bottom of the image and the code above it. The loader tests check that
   order, so it is spelled out here: .rodata at 0x10000, .text on the next
   page and .data/.bss on the page after that, each in its own PT_LOAD.
   0x10000 keeps the whole image inside the 1 MiB RAM the tests give it. */
OUTPUT_ARCH( "riscv" )
ENTRY( _start )

PHDRS
{
  rodata PT_LOAD FLAGS(4);    /* R   */
  text   PT_LOAD FLAGS(5);    /* R X */
  data   PT_LOAD FLAGS(6);    /* R W */
}

SECTIONS
{
  . = 0x00010000;
  .rodata : { *(.rodata .rodata.*) } :rodata
  . = ALIGN(0x1000);
  .text : { *(.text .text.*) } :text
  . = ALIGN(0x1000);
  .data : { *(.data .data.*) } :data
  .bss : { *(.bss .bss.*) } :data
}
