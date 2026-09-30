use core::arch::asm;

pub fn delay_microseconds(time: i32) {
    // Calculate the approximate number of CPU cycles that need to be burned,
    // assuming a 33.8688 MHz clock (1 us = 33.8688 = ~33.875 = 271 / 8 cycles).
    burn_cycles((time.wrapping_mul(271) + 4) / 8);
}

/// Each loop iteration (a branch and a decrement) burns 2 cycles.
#[inline(always)]
pub fn burn_cycles(cycles: i32) {
    unsafe {
        asm!(
            ".set push",
            ".set noreorder",
            ".set noat",
            "1:",
            "bgtz {cycles}, 1b",
            "addiu {cycles}, {cycles}, -2",
            ".set pop",
            cycles = inout(reg) cycles => _,
            options(nomem, nostack),
        );
    }
}
