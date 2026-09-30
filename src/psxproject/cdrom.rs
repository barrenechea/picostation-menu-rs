use crate::aligned::Aligned;
use crate::cell::{Volatile, compiler_barrier};
use crate::ps1::cdrom::*;
use crate::ps1::registers::*;

use super::delay::{burn_cycles, delay_microseconds};

static WAITING_FOR_INT1: Volatile<bool> = Volatile::new(false);
static WAITING_FOR_INT2: Volatile<bool> = Volatile::new(false);
static WAITING_FOR_INT3: Volatile<bool> = Volatile::new(false);
static WAITING_FOR_INT4: Volatile<bool> = Volatile::new(false);
static WAITING_FOR_INT5: Volatile<bool> = Volatile::new(false);

static READ_DATA_PTR: Volatile<usize> = Volatile::new(0);
static READ_DATA_SECTOR_SIZE: Volatile<usize> = Volatile::new(0);
static READ_DATA_NUM_SECTORS: Volatile<usize> = Volatile::new(0);

static RESPONSE: [Volatile<u8>; 16] = [const { Volatile::new(0) }; 16];
static RESP_LENGTH: Volatile<u8> = Volatile::new(0);
static STATUS: Volatile<u8> = Volatile::new(0);

fn is_busy() -> bool {
    CDROM_HSTS.read() & CDROM_HSTS_BUSYSTS != 0
}

pub fn init_cdrom() {
    BIU_DEV5_CTRL.write(0x0002_0943);
    DMA_DPCR.set_bits(dma_dpcr_ch_enable(DMA_CDROM));

    CDROM_ADDRESS.write(1);
    // Acknowledge and enable all IRQs.
    CDROM_HCLRCTL.write(CDROM_HCLRCTL_CLRINT0 | CDROM_HCLRCTL_CLRINT1 | CDROM_HCLRCTL_CLRINT2);
    CDROM_HINTMSK_W.write(CDROM_HCLRCTL_CLRINT0 | CDROM_HCLRCTL_CLRINT1 | CDROM_HCLRCTL_CLRINT2);

    CDROM_ADDRESS.write(0);
    CDROM_HCHPCTL.write(0);

    // Send the left and right audio channels to the matching SPU channels.
    CDROM_ADDRESS.write(2);
    CDROM_ATV0.write(128);
    CDROM_ATV1.write(0);

    CDROM_ADDRESS.write(3);
    CDROM_ATV2.write(128);
    CDROM_ATV3.write(0);
    CDROM_ADPCTL.write(CDROM_ADPCTL_CHNGATV);
}

pub fn issue_cdrom_command(cmd: u8, args: &[u8]) {
    WAITING_FOR_INT1.set(true);
    WAITING_FOR_INT2.set(true);
    WAITING_FOR_INT3.set(true);
    WAITING_FOR_INT4.set(true);
    WAITING_FOR_INT5.set(true);
    STATUS.set(0);

    while is_busy() {}

    CDROM_ADDRESS.write(1);
    CDROM_HCLRCTL.write(CDROM_HCLRCTL_CLRPRM);
    delay_microseconds(100);
    burn_cycles(102);

    while is_busy() {}

    CDROM_ADDRESS.write(0);
    for &arg in args {
        CDROM_PARAMETER.write(arg);
    }

    CDROM_COMMAND.write(cmd);
}

fn wait_for_int2() {
    while WAITING_FOR_INT2.get() && WAITING_FOR_INT5.get() {}
}

fn wait_for_int3() {
    while WAITING_FOR_INT3.get() && WAITING_FOR_INT5.get() {}
}

/// Reads `num_sectors` sectors starting at `lba` into `ptr`, blocking until
/// they have all been read if `wait` is set. Returns early on a drive error.
///
/// # Safety
///
/// `ptr` must be word-aligned and valid for `num_sectors * sector_size` bytes
/// until the read completes.
pub unsafe fn start_cdrom_read(
    lba: u32,
    ptr: *mut u8,
    num_sectors: usize,
    sector_size: usize,
    double_speed: bool,
    wait: bool,
) {
    // Earlier writes to the buffer must not land after the DMA fills it.
    compiler_barrier();
    READ_DATA_PTR.set(ptr.expose_provenance());
    READ_DATA_NUM_SECTORS.set(num_sectors);
    READ_DATA_SECTOR_SIZE.set(sector_size);

    let mut mode = 0;

    if sector_size == 2340 {
        mode |= CDROM_MODE_SIZE_2340;
    }
    if double_speed {
        mode |= CDROM_MODE_SPEED_2X;
    }

    let msf = cdrom_convert_lba_to_msf(lba);
    delay_microseconds(100);

    issue_cdrom_command(CDROM_CMD_SETMODE, &[mode]);
    wait_for_int3();
    issue_cdrom_command(CDROM_CMD_SETLOC, &msf.as_bytes());
    wait_for_int3();
    issue_cdrom_command(CDROM_CMD_READ_N, &[]);
    wait_for_int3();

    if wait {
        wait_for_int1();
    }

    // The buffer is written by DMA, which the compiler can't see.
    compiler_barrier();
}

/// Waits for the requested sectors to be read, unless the drive reports an
/// error.
fn wait_for_int1() {
    while WAITING_FOR_INT5.get() && WAITING_FOR_INT1.get() {
        delay_microseconds(100);
    }
}

pub fn update_cdrom_toc() {
    issue_cdrom_command(CDROM_CMD_SETSESSION, &[1]);

    wait_for_int3();
    wait_for_int2();
}

pub fn is_playstation_cd() -> bool {
    let mut buffer = Aligned([0u8; 2048]);

    unsafe { start_cdrom_read(16, buffer.0.as_mut_ptr(), 1, 2048, true, true) };
    buffer.0[8..19] == *b"PLAYSTATION"
}

pub(super) fn read_response() {
    RESP_LENGTH.set(0);

    while CDROM_HSTS.read() & CDROM_HSTS_RSLRRDY != 0 {
        let length = RESP_LENGTH.get();
        let result = CDROM_RESULT.read();

        if let Some(byte) = RESPONSE.get(length as usize) {
            byte.set(result);
        }
        RESP_LENGTH.set(length.wrapping_add(1));
    }
}

/// A sector is ready to be transferred via DMA. Pauses the drive after the
/// last sector.
pub(super) fn cdrom_int1() {
    let ptr = READ_DATA_PTR.get();
    let sector_size = READ_DATA_SECTOR_SIZE.get();

    dma_madr(DMA_CDROM).write(ptr as u32);
    dma_bcr(DMA_CDROM).write((sector_size / 4) as u32);
    dma_chcr(DMA_CDROM).write(DMA_CHCR_ENABLE | DMA_CHCR_TRIGGER);

    compiler_barrier();
    READ_DATA_PTR.set(ptr + sector_size);

    let remaining = READ_DATA_NUM_SECTORS.get().wrapping_sub(1);

    READ_DATA_NUM_SECTORS.set(remaining);
    if remaining == 0 {
        issue_cdrom_command(CDROM_CMD_PAUSE, &[]);
    }

    compiler_barrier();
    WAITING_FOR_INT1.set(false);
}

pub(super) fn cdrom_int2() {
    WAITING_FOR_INT2.set(false);
}

pub(super) fn cdrom_int3() {
    STATUS.set(RESPONSE[0].get());
    WAITING_FOR_INT3.set(false);
}

pub(super) fn cdrom_int4() {
    WAITING_FOR_INT4.set(false);
}

/// The drive reported an error.
pub(super) fn cdrom_int5() {
    WAITING_FOR_INT5.set(false);
}
