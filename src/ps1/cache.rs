// ps1-bare-metal - (C) 2023-2025 spicyjpeg
//
// Permission to use, copy, modify, and/or distribute this software for any
// purpose with or without fee is hereby granted, provided that the above
// copyright notice and this permission notice appear in all copies.
//
// THE SOFTWARE IS PROVIDED "AS IS" AND THE AUTHOR DISCLAIMS ALL WARRANTIES WITH
// REGARD TO THIS SOFTWARE INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY
// AND FITNESS. IN NO EVENT SHALL THE AUTHOR BE LIABLE FOR ANY SPECIAL, DIRECT,
// INDIRECT, OR CONSEQUENTIAL DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM
// LOSS OF USE, DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR
// OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION WITH THE USE OR
// PERFORMANCE OF THIS SOFTWARE.

//! Instruction cache flush. Much faster than the BIOS implementation, as it
//! only clears the tags and doesn't run from the ROM's 8-bit bus.

use core::arch::naked_asm;

#[unsafe(naked)]
#[unsafe(export_name = "flushCache")]
pub unsafe extern "C" fn flush_cache() {
    naked_asm!(
        r#"
    .set push
    .set noreorder
    .set noat

    # Call _flushCacheInner() through the uncached KSEG1 mirror of main RAM,
    # ensuring the CPU will not attempt to use the cache while it is being
    # cleared. This jump must be performed using a la/jr combo as immediate
    # jumps (j) only update the program counter's bottommost 28 bits.
    lui   $a0, %hi(_flushCacheInner)
    addiu $a0, $a0, %lo(_flushCacheInner)
    lui   $a1, 0xa000
    or    $a0, $a0, $a1

    jr    $a0
    lui   $a0, 0xfffe # %hi(CPU_BCC)

_flushCacheInner:
    # Save the current state of the BCC and COP0 status registers so that they
    # can be restored later.
    mfc0  $a2, $12
    lw    $a3, 0x0130($a0) # %lo(CPU_BCC)

    # Disable interrupts and the scratchpad, put the instruction cache into "tag
    # test" mode and proceed to map it directly to the CPU's address space by
    # setting the COP0 "isolate cache" flag.
    mtc0  $zero, $12

    # CPU_BCC = (CPU_BCC & ~CPU_BCC_DS) | CPU_BCC_TAG | CPU_BCC_IS1;
    addiu $a1, $zero, -129 # ~CPU_BCC_DS
    and   $a1, $a1, $a3
    ori   $a1, $a1, 0x0804 # CPU_BCC_TAG | CPU_BCC_IS1
    sw    $a1, 0x0130($a0)

    # cop0_setReg(COP0_STATUS, COP0_STATUS_IsC);
    lui   $a1, 0x0001
    mtc0  $a1, $12

    # Use an unrolled loop to clear all tags, thus invalidating the cache's
    # contents. "Tag test" mode maps each tag into memory at the offset of its
    # respective 16-byte cache line.
    addiu $a1, $zero, 0x1000 - 256

.LclearLoop: # for (int i = 0x1000 - 256; i >= 0; i -= 256)
    sw    $zero, 0x00($a1)
    sw    $zero, 0x10($a1)
    sw    $zero, 0x20($a1)
    sw    $zero, 0x30($a1)
    sw    $zero, 0x40($a1)
    sw    $zero, 0x50($a1)
    sw    $zero, 0x60($a1)
    sw    $zero, 0x70($a1)
    sw    $zero, 0x80($a1)
    sw    $zero, 0x90($a1)
    sw    $zero, 0xa0($a1)
    sw    $zero, 0xb0($a1)
    sw    $zero, 0xc0($a1)
    sw    $zero, 0xd0($a1)
    sw    $zero, 0xe0($a1)
    sw    $zero, 0xf0($a1)

    bgtz  $a1, .LclearLoop
    addiu $a1, $a1, -256

    # Clear the "isolate cache" bit, restore the previously saved state and
    # return.
    mtc0  $zero, $12
    nop
    sw    $a3, 0x0130($a0)
    mtc0  $a2, $12

    jr    $ra
    nop

    .set pop
"#
    )
}
