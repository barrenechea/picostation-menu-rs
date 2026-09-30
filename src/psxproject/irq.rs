use core::ffi::c_void;
use core::ptr;

use crate::cell::Volatile;
use crate::ps1::cdrom::*;
use crate::ps1::registers::*;

use super::cdrom;
use super::delay::delay_microseconds;
use super::system::{
    acknowledge_interrupt, enable_interrupts, install_exception_handler, set_interrupt_handler,
};

static VBLANK: Volatile<bool> = Volatile::new(false);

fn handle_vsync_irq() {
    VBLANK.set(true);
}

fn handle_cdrom_irq() {
    // The interrupted code may be between selecting a bank and using it.
    let bank = CDROM_HSTS.read() & 3;

    CDROM_ADDRESS.write(1);

    let irq_type = CDROM_HINTSTS.read() & (CDROM_HINT_INT0 | CDROM_HINT_INT1 | CDROM_HINT_INT2);

    // If a new sector is available, request a sector buffer read.
    if irq_type == CDROM_IRQ_DATA_READY {
        CDROM_ADDRESS.write(0);
        CDROM_HCHPCTL.write(0);
        CDROM_HCHPCTL.write(CDROM_HCHPCTL_BFRD);
    }

    CDROM_ADDRESS.write(1);
    CDROM_HCLRCTL.write(CDROM_HCLRCTL_CLRINT0 | CDROM_HCLRCTL_CLRINT1 | CDROM_HCLRCTL_CLRINT2);
    CDROM_HCLRCTL.write(CDROM_HCLRCTL_CLRPRM);
    delay_microseconds(3);

    cdrom::read_response();

    match irq_type {
        CDROM_IRQ_DATA_READY => cdrom::cdrom_int1(),
        CDROM_IRQ_COMPLETE => cdrom::cdrom_int2(),
        CDROM_IRQ_ACKNOWLEDGE => cdrom::cdrom_int3(),
        CDROM_IRQ_DATA_END => cdrom::cdrom_int4(),
        CDROM_IRQ_ERROR => cdrom::cdrom_int5(),
        _ => {}
    }

    CDROM_ADDRESS.write(bank);
}

unsafe extern "C" fn interrupt_handler_function(_arg: *mut c_void) {
    if acknowledge_interrupt(IRQ_VSYNC) {
        handle_vsync_irq();
    }
    if acknowledge_interrupt(IRQ_CDROM) {
        handle_cdrom_irq();
    }
    // The menu never streams audio, so SPU interrupts need no handling.
    acknowledge_interrupt(IRQ_SPU);
}

pub fn init_irq() {
    install_exception_handler();
    set_interrupt_handler(interrupt_handler_function, ptr::null_mut());
    IRQ_MASK.write((1 << IRQ_VSYNC) | (1 << IRQ_CDROM) | (1 << IRQ_SPU));
    enable_interrupts();
}

pub fn wait_for_vblank() {
    while !VBLANK.get() {}
    VBLANK.set(false);
}
