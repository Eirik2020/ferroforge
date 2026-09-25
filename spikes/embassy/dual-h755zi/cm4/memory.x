/* EXPERIMENT: STM32H755ZI, Cortex-M4 image. Flash bank 2, D2 SRAM1-3.
   SRAM4 (D3) is shared with the M7 image and must match its memory.x. */
MEMORY
{
  FLASH  : ORIGIN = 0x08100000, LENGTH = 1024K
  RAM    : ORIGIN = 0x30000000, LENGTH = 288K
  RAM_D3 : ORIGIN = 0x38000000, LENGTH = 64K
}

/* The first 1K is embassy's SharedData, the next the mailbox. Symbols, not
   sections, so neither image's optimizer can see into what the other writes. */
__spike_embassy_shared_data = ORIGIN(RAM_D3);
__spike_mailbox = ORIGIN(RAM_D3) + 0x400;
