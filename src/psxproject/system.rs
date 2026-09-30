use core::arch::naked_asm;
use core::ffi::c_void;
use core::ptr;

use crate::cell::{StaticCell, Volatile, compiler_barrier};
use crate::ps1::cache::flush_cache;
use crate::ps1::cop0::{self, *};
use crate::ps1::registers::*;

use super::delay::delay_microseconds;

const BIOS_ENTRY_POINT: usize = 0xbfc0_0000;
const BIOS_ALT_ENTRY_POINT: usize = 0xbfc0_0390;
const BIOS_API_TABLE: usize = 0x8000_0200;
const BIOS_SHELL_LOAD_ADDR: usize = 0x8003_0000;
const BIOS_BREAK_VECTOR: usize = 0x8000_0040;
const BIOS_EXC_VECTOR: usize = 0x8000_0080;
const BIOS_SIGNATURE: usize = 0xbfc0_0108;

pub type ArgFunction = unsafe extern "C" fn(arg: *mut c_void);

/// Register state saved by the exception handler: pc, then $at to $ra in
/// order (skipping $zero, $k0 and $k1), then $hi and $lo.
#[repr(C)]
pub struct Thread {
    registers: [u32; 32],
}

static MAIN_THREAD: StaticCell<Thread> = StaticCell::new(Thread { registers: [0; 32] });

static CURRENT_THREAD: Volatile<*mut Thread> = Volatile::new(MAIN_THREAD.get());
static NEXT_THREAD: Volatile<*mut Thread> = Volatile::new(MAIN_THREAD.get());

static INTERRUPT_HANDLER: Volatile<Option<ArgFunction>> = Volatile::new(None);
static INTERRUPT_HANDLER_ARG: Volatile<*mut c_void> = Volatile::new(ptr::null_mut());

/// Copied to the exception vector (0x80000080). It can only use $k0 and $k1, as
/// any other register belongs to the interrupted thread.
#[unsafe(naked)]
unsafe extern "C" fn exception_vector() {
    naked_asm!(
        r#"
    .set push
    .set noreorder
    .set noat

    lui   $k0, %hi({current_thread})
    lw    $k0, %lo({current_thread})($k0)

    j     {exception_handler}
    mfc0  $k1, $14

    .set pop
"#,
        current_thread = sym CURRENT_THREAD,
        exception_handler = sym exception_handler,
    )
}

#[unsafe(naked)]
unsafe extern "C" fn exception_handler() {
    naked_asm!(
        r#"
    .set push
    .set noreorder
    .set noat

    # $hi/$lo are saved last to let the multiplier finish any ongoing
    # calculation.
    sw    $at, 0x04($k0)
    sw    $v0, 0x08($k0)
    sw    $v1, 0x0c($k0)
    sw    $a0, 0x10($k0)
    sw    $a1, 0x14($k0)
    sw    $a2, 0x18($k0)
    sw    $a3, 0x1c($k0)
    sw    $t0, 0x20($k0)
    sw    $t1, 0x24($k0)
    sw    $t2, 0x28($k0)
    sw    $t3, 0x2c($k0)
    sw    $t4, 0x30($k0)
    sw    $t5, 0x34($k0)
    sw    $t6, 0x38($k0)
    sw    $t7, 0x3c($k0)
    sw    $s0, 0x40($k0)
    sw    $s1, 0x44($k0)
    sw    $s2, 0x48($k0)
    sw    $s3, 0x4c($k0)
    sw    $s4, 0x50($k0)
    sw    $s5, 0x54($k0)
    sw    $s6, 0x58($k0)
    sw    $s7, 0x5c($k0)
    sw    $t8, 0x60($k0)
    sw    $t9, 0x64($k0)
    sw    $gp, 0x68($k0)
    sw    $sp, 0x6c($k0)
    sw    $fp, 0x70($k0)
    sw    $ra, 0x74($k0)

    mfhi  $v0
    mflo  $v1
    sw    $v0, 0x78($k0)
    sw    $v1, 0x7c($k0)

    # Bits 2-6 of CAUSE hold the exception code: 0 for interrupts, 8 for
    # syscalls (whose EPC must be incremented to avoid running them again).
    mfc0  $v0, $13
    lui   $v1, %hi({interrupt_handler})

    andi  $v0, $v0, 0x7c
    beqz  $v0, .LcheckForGTEInst
    addiu $at, $zero, 0x20
    beq   $v0, $at, .LapplyIncrement
    lw    $v1, %lo({interrupt_handler})($v1)

.LotherException:
    sw    $k1, 0x00($k0)

    mfc0  $a1, $8
    srl   $a0, $v0, 2
    jal   {unhandled_exception}
    addiu $sp, $sp, -8

    b     .Lreturn
    addiu $sp, $sp, 8

.LcheckForGTEInst:
    # Work around a hardware bug: if the interrupted instruction was a GTE
    # opcode, skip it as it has already been executed.
    lw    $v0, 0($k1)
    addiu $at, $zero, 0x25
    srl   $v0, $v0, 25
    bne   $v0, $at, .LskipIncrement
    lw    $v1, %lo({interrupt_handler})($v1)

.LapplyIncrement:
    addiu $k1, $k1, 4

.LskipIncrement:
    # The interrupt handler temporarily uses the current thread's stack.
    sw    $k1, 0x00($k0)

    lui   $a0, %hi({interrupt_handler_arg})
    lw    $a0, %lo({interrupt_handler_arg})($a0)
    jalr  $v1
    addiu $sp, $sp, -8

    addiu $sp, $sp, 8

.Lreturn:
    lui   $k0, %hi({next_thread})
    lw    $k0, %lo({next_thread})($k0)
    lui   $at, %hi({current_thread})
    sw    $k0, %lo({current_thread})($at)

    lw    $v0, 0x78($k0)
    lw    $v1, 0x7c($k0)
    mthi  $v0
    mtlo  $v1

    lw    $k1, 0x00($k0)
    lw    $at, 0x04($k0)
    lw    $v0, 0x08($k0)
    lw    $v1, 0x0c($k0)
    lw    $a0, 0x10($k0)
    lw    $a1, 0x14($k0)
    lw    $a2, 0x18($k0)
    lw    $a3, 0x1c($k0)
    lw    $t0, 0x20($k0)
    lw    $t1, 0x24($k0)
    lw    $t2, 0x28($k0)
    lw    $t3, 0x2c($k0)
    lw    $t4, 0x30($k0)
    lw    $t5, 0x34($k0)
    lw    $t6, 0x38($k0)
    lw    $t7, 0x3c($k0)
    lw    $s0, 0x40($k0)
    lw    $s1, 0x44($k0)
    lw    $s2, 0x48($k0)
    lw    $s3, 0x4c($k0)
    lw    $s4, 0x50($k0)
    lw    $s5, 0x54($k0)
    lw    $s6, 0x58($k0)
    lw    $s7, 0x5c($k0)
    lw    $t8, 0x60($k0)
    lw    $t9, 0x64($k0)
    lw    $gp, 0x68($k0)
    lw    $sp, 0x6c($k0)
    lw    $fp, 0x70($k0)
    lw    $ra, 0x74($k0)

    jr    $k1
    .word 0x42000010 # rfe, which LLVM's assembler lacks

    .set pop
"#,
        current_thread = sym CURRENT_THREAD,
        next_thread = sym NEXT_THREAD,
        interrupt_handler = sym INTERRUPT_HANDLER,
        interrupt_handler_arg = sym INTERRUPT_HANDLER_ARG,
        unhandled_exception = sym unhandled_exception,
    )
}

/// Copied to the COP0 breakpoint vector (0x80000040) for a fast reboot. The
/// breakpoint trips when the BIOS copies the first byte of the shell to RAM;
/// this removes the breakpoint, undoes the write and forces the copying function
/// to return before anything else is copied.
#[unsafe(naked)]
unsafe extern "C" fn fast_reboot_break_vector() {
    naked_asm!(
        r#"
    .set push
    .set noreorder

    mtc0  $zero, $7
    sw    $zero, -1($a0)
    jr    $ra
    .word 0x42000010 # rfe

    .set pop
"#
    )
}

/// Stands in for the shell the BIOS "loaded": once it returns, the kernel boots
/// the CD-ROM. The first instruction is overwritten with a nop by
/// `fast_reboot_break_vector`.
#[unsafe(naked)]
unsafe extern "C" fn fast_reboot_dummy_shell() {
    naked_asm!(
        r#"
    .set push
    .set noreorder

    nop
    jr    $ra
    nop
    nop

    .set pop
"#
    )
}

extern "C" fn unhandled_exception(_cause: i32, _badv: u32) -> ! {
    loop {
        core::hint::spin_loop();
    }
}

/// Copies one of the 16-byte assembly stubs to the given address.
fn copy_stub(destination: usize, stub: unsafe extern "C" fn()) {
    let source = stub as *const u32;
    let destination = ptr::without_provenance_mut::<u32>(destination);

    for i in 0..4 {
        unsafe {
            ptr::write_volatile(
                destination.wrapping_add(i),
                ptr::read_volatile(source.wrapping_add(i)),
            )
        };
    }
}

fn jump(address: usize) -> ! {
    let entry_point: extern "C" fn() -> ! = unsafe { core::mem::transmute(address) };

    entry_point()
}

pub fn reset_interrupts() {
    // Disable interrupts and enable the GTE.
    cop0::set_reg::<COP0_STATUS>(COP0_STATUS_CU0 | COP0_STATUS_CU2);

    IRQ_MASK.write(0);
    IRQ_STAT.write(0);
    DMA_DPCR.write(0);
    DMA_DICR.write(DMA_DICR_CH_STAT_BITMASK);
}

pub fn install_exception_handler() {
    IRQ_MASK.write(0);
    IRQ_STAT.write(0);
    DMA_DPCR.write(0);
    DMA_DICR.write(DMA_DICR_CH_STAT_BITMASK);

    // Disable interrupts and the GTE.
    cop0::set_reg::<COP0_STATUS>(COP0_STATUS_CU0);

    // The BIOS' cache flush function must run from ROM, as it temporarily
    // disables main RAM.
    let bios_flush_cache: extern "C" fn() = unsafe {
        core::mem::transmute(ptr::read_volatile(
            ptr::without_provenance::<usize>(BIOS_API_TABLE).wrapping_add(0x44),
        ))
    };

    copy_stub(BIOS_BREAK_VECTOR, exception_vector);
    copy_stub(BIOS_EXC_VECTOR, exception_vector);
    bios_flush_cache();

    DMA_DPCR.write(0x0bbb_bbbb);
    DMA_DICR.write(DMA_DICR_IRQ_ENABLE);

    cop0::set_reg::<COP0_STATUS>(
        COP0_STATUS_IEC | COP0_STATUS_IM2 | COP0_STATUS_CU0 | COP0_STATUS_CU2,
    );
}

pub fn set_interrupt_handler(function: ArgFunction, arg: *mut c_void) {
    disable_interrupts();

    INTERRUPT_HANDLER.set(Some(function));
    INTERRUPT_HANDLER_ARG.set(arg);
    compiler_barrier();
}

pub fn enable_interrupts() {
    cop0::set_reg::<COP0_STATUS>(cop0::get_reg::<COP0_STATUS>() | COP0_STATUS_IEC);
}

/// Returns whether interrupts were enabled.
pub fn disable_interrupts() -> bool {
    let status = cop0::get_reg::<COP0_STATUS>();

    cop0::set_reg::<COP0_STATUS>(status & !COP0_STATUS_IEC);
    status & COP0_STATUS_IEC != 0
}

pub fn soft_reset() -> ! {
    disable_interrupts();
    jump(BIOS_ENTRY_POINT)
}

/// Reboots into the disc, skipping the BIOS shell.
pub fn soft_fast_reboot() -> ! {
    reset_interrupts();

    // The hack only works with Sony's kernel, so fall back to a full reset on
    // custom BIOSes.
    let signature = b"Sony Computer Entertainment Inc.";
    let bios_signature = ptr::without_provenance::<u8>(BIOS_SIGNATURE);

    for (i, &byte) in signature.iter().enumerate() {
        if unsafe { ptr::read_volatile(bios_signature.wrapping_add(i)) } != byte {
            jump(BIOS_ENTRY_POINT);
        }
    }

    // Place a dummy shell where the BIOS loads the actual shell, and protect
    // it with a breakpoint on the first write to 0x80030000-0x8003ffff.
    copy_stub(BIOS_BREAK_VECTOR, fast_reboot_break_vector);
    copy_stub(BIOS_SHELL_LOAD_ADDR, fast_reboot_dummy_shell);
    unsafe { flush_cache() };

    cop0::set_reg::<COP0_DCIC>(0);
    cop0::set_reg::<COP0_BDA>(BIOS_SHELL_LOAD_ADDR as u32);
    cop0::set_reg::<COP0_BDAM>(0xffff_0000);
    cop0::set_reg::<COP0_DCIC>(
        COP0_DCIC_DE | COP0_DCIC_DAE | COP0_DCIC_DW | COP0_DCIC_KD | COP0_DCIC_UD | COP0_DCIC_TR,
    );

    // Skip the part of the BIOS entry point that clears COP0 registers (and
    // would thus disable the breakpoint).
    jump(BIOS_ALT_ENTRY_POINT)
}

pub fn acknowledge_interrupt(irq: u32) -> bool {
    if IRQ_STAT.read() & (1 << irq) != 0 {
        IRQ_STAT.write(!(1 << irq));
        return true;
    }

    false
}

pub fn wait_for_dma_transfer(dma: usize, mut timeout: i32) -> bool {
    while timeout > 0 {
        if dma_chcr(dma).read() & DMA_CHCR_ENABLE == 0 {
            return true;
        }

        delay_microseconds(10);
        timeout -= 10;
    }

    false
}
