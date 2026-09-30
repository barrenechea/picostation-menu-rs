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

//! Coprocessor 0 (system control) registers.

use core::arch::asm;

pub const COP0_BDA: u32 = 5; // Breakpoint data address
pub const COP0_DCIC: u32 = 7; // Debug and cache invalidation control
pub const COP0_BDAM: u32 = 9; // Breakpoint data address mask
pub const COP0_STATUS: u32 = 12; // Status register

pub const COP0_DCIC_DE: u32 = 1 << 23; // Debug enable
pub const COP0_DCIC_DAE: u32 = 1 << 25; // Data address breakpoint enable
pub const COP0_DCIC_DW: u32 = 1 << 27; // Data address write breakpoint enable
pub const COP0_DCIC_KD: u32 = 1 << 29; // Kernel debug enable
pub const COP0_DCIC_UD: u32 = 1 << 30; // User debug enable
pub const COP0_DCIC_TR: u32 = 1 << 31; // Debug event trap enable

pub const COP0_STATUS_IEC: u32 = 1 << 0; // Current interrupt enable
pub const COP0_STATUS_IM2: u32 = 1 << 10; // IRQ mask 2 (hardware interrupt)
pub const COP0_STATUS_CU0: u32 = 1 << 28; // Coprocessor 0 privilege level
pub const COP0_STATUS_CU2: u32 = 1 << 30; // Coprocessor 2 enable

// LLVM may allocate $at to an operand, hence the .set noat.

#[inline(always)]
pub fn set_reg<const REG: u32>(value: u32) {
    unsafe {
        asm!(
            ".set push",
            ".set noat",
            "mtc0 {value}, ${reg}",
            ".set pop",
            value = in(reg) value,
            reg = const REG,
            options(nostack),
        )
    }
}

#[inline(always)]
pub fn get_reg<const REG: u32>() -> u32 {
    let value: u32;

    // mfc0 has a load delay slot, and the compiler doesn't know about it.
    unsafe {
        asm!(
            ".set push",
            ".set noat",
            "mfc0 {value}, ${reg}",
            "nop",
            ".set pop",
            value = out(reg) value,
            reg = const REG,
            options(nostack),
        )
    }
    value
}
