/* EXPERIMENT: STM32H755ZI, Cortex-M7 image. Flash bank 1, AXI SRAM.
   SRAM4 (D3) is shared with the M4 image and must match its memory.x. */
MEMORY
{
  FLASH  : ORIGIN = 0x08000000, LENGTH = 1024K
  RAM    : ORIGIN = 0x24000000, LENGTH = 512K
  RAM_D3 : ORIGIN = 0x38000000, LENGTH = 64K
}

/* The first 1K is embassy's SharedData, the next the mailbox. Symbols, not
   sections, so neither image's optimizer can see into what the other writes. */
__spike_embassy_shared_data = ORIGIN(RAM_D3);
__spike_mailbox = ORIGIN(RAM_D3) + 0x400;
