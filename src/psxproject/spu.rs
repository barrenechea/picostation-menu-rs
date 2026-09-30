// Adapted by Rhys Baker
// Based on the C++ code at https://github.com/spicyjpeg/573in1/blob/dev/src/common/spu.hpp

use crate::cell::{Volatile, compiler_barrier};
use crate::ps1::registers::*;

use super::delay::delay_microseconds;
use super::system::wait_for_dma_transfer;

const DMA_CHUNK_SIZE: usize = 4;
const DMA_TIMEOUT: i32 = 100_000;
const STATUS_TIMEOUT: i32 = 10_000;

const NUM_CHANNELS: usize = 24;
const DUMMY_BLOCK_OFFSET: u32 = 0x1000;
const SPU_RAM_END: u32 = 0x7fff0;

pub const MAX_VOLUME: u16 = 0x3fff;

/// Size of the VAG file header preceding the sample data.
const VAG_HEADER_SIZE: usize = 48;

/// Next free address in SPU RAM, past the dummy block.
static SPU_ALLOC_PTR: Volatile<u32> = Volatile::new(0x1010);

/// A sound uploaded to SPU RAM. An offset of 0 means it failed to load.
#[derive(Clone, Copy, Default)]
pub struct Sound {
    offset: u32,
    sample_rate: u16,
    length: u16,
}

fn wait_for_status(mask: u16, value: u16) -> bool {
    let mut timeout = STATUS_TIMEOUT;

    while timeout > 0 {
        if SPU_STAT.read() & mask == value {
            return true;
        }

        delay_microseconds(10);
        timeout -= 10;
    }

    false
}

pub fn init_spu() {
    BIU_DEV4_CTRL.write(
        1 // Write delay
            | (14 << 4) // Read delay
            | BIU_CTRL_RECOVERY
            | BIU_CTRL_WIDTH_16
            | BIU_CTRL_AUTO_INCR
            | (9 << 16) // Number of address lines
            | BIU_CTRL_DMA_DELAY,
    );

    SPU_CTRL.write(0);
    wait_for_status(0x3f, 0);

    SPU_MASTER_VOL_L.write(0);
    SPU_MASTER_VOL_R.write(0);
    SPU_REVERB_VOL_L.write(0);
    SPU_REVERB_VOL_R.write(0);
    SPU_REVERB_ADDR.write((SPU_RAM_END / 8) as u16);

    SPU_FLAG_FM1.write(0);
    SPU_FLAG_FM2.write(0);
    SPU_FLAG_NOISE1.write(0);
    SPU_FLAG_NOISE2.write(0);
    SPU_FLAG_REVERB1.write(0);
    SPU_FLAG_REVERB2.write(0);

    SPU_CTRL.write(SPU_CTRL_ENABLE);
    wait_for_status(0x3f, 0);

    // Place a dummy (silent) looping block at the beginning of SPU RAM.
    SPU_DMA_CTRL.write(4);
    SPU_ADDR.write((DUMMY_BLOCK_OFFSET / 8) as u16);

    SPU_DATA.write(0x0500);
    for _ in 0..7 {
        SPU_DATA.write(0);
    }

    SPU_CTRL.write(SPU_CTRL_XFER_WRITE | SPU_CTRL_ENABLE);
    wait_for_status(SPU_CTRL_XFER_BITMASK | SPU_STAT_BUSY, SPU_CTRL_XFER_WRITE);
    delay_microseconds(100);

    SPU_CTRL.write(SPU_CTRL_UNMUTE | SPU_CTRL_ENABLE);
    stop_channels(0);

    DMA_DPCR.set_bits(dma_dpcr_ch_enable(DMA_SPU));

    set_master_volume(MAX_VOLUME, 0);
}

fn set_master_volume(master: u16, reverb: u16) {
    SPU_MASTER_VOL_L.write(master);
    SPU_MASTER_VOL_R.write(master);
    SPU_REVERB_VOL_L.write(reverb);
    SPU_REVERB_VOL_R.write(reverb);
}

pub fn stop_channels(mask: u32) {
    let mask = mask & ((1 << NUM_CHANNELS) - 1);

    SPU_FLAG_OFF1.write(mask as u16);
    SPU_FLAG_OFF2.write((mask >> 16) as u16);

    for ch in (0..NUM_CHANNELS).filter(|ch| mask & (1 << ch) != 0) {
        spu_ch_vol_l(ch).write(0);
        spu_ch_vol_r(ch).write(0);
        spu_ch_freq(ch).write(1 << 12);
        spu_ch_addr(ch).write((DUMMY_BLOCK_OFFSET / 8) as u16);
    }

    SPU_FLAG_ON1.write(mask as u16);
    SPU_FLAG_ON2.write((mask >> 16) as u16);
}

/// Uploads `length` bytes, rounded up to whole DMA chunks, to SPU RAM. Returns
/// the number of bytes uploaded.
fn upload(offset: u32, data: &[u8], length: usize, wait: bool) -> usize {
    let length = (length / 4).div_ceil(DMA_CHUNK_SIZE);

    if !wait_for_dma_transfer(DMA_SPU, DMA_TIMEOUT) {
        return 0;
    }

    let ctrl_reg = SPU_CTRL.read() & !SPU_CTRL_XFER_BITMASK;

    SPU_CTRL.write(ctrl_reg);
    wait_for_status(SPU_CTRL_XFER_BITMASK, 0);

    SPU_DMA_CTRL.write(4);
    SPU_ADDR.write((offset / 8) as u16);
    SPU_CTRL.write(ctrl_reg | SPU_CTRL_XFER_DMA_WRITE);
    wait_for_status(SPU_CTRL_XFER_BITMASK, SPU_CTRL_XFER_DMA_WRITE);

    compiler_barrier();
    dma_madr(DMA_SPU).write(data.as_ptr().expose_provenance() as u32);
    dma_bcr(DMA_SPU).write(DMA_CHUNK_SIZE as u32 | ((length as u32 & 0xffff) << 16));
    dma_chcr(DMA_SPU).write(DMA_CHCR_WRITE | DMA_CHCR_MODE_SLICE | DMA_CHCR_ENABLE);

    if wait {
        wait_for_dma_transfer(DMA_SPU, DMA_TIMEOUT);
    }

    length * DMA_CHUNK_SIZE * 4
}

/// Uploads a mono VAG file to SPU RAM.
pub fn sound_load_sound_from_binary(data: &[u8]) -> Sound {
    let field = |offset: usize| {
        u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ])
    };

    let magic = field(0);
    let length = field(12).swap_bytes();
    let channels = u16::from_le_bytes([data[30], data[31]]);

    // The (big-endian) sample rate field is scaled before being byte-swapped,
    // which is how the menu's sounds have always been pitched.
    let sample_rate = (field(16).wrapping_mul(2) / 3).swap_bytes();

    if magic != u32::from_le_bytes(*b"VAGp") || channels > 1 {
        return Sound::default();
    }

    let sound = Sound {
        offset: SPU_ALLOC_PTR.get(),
        sample_rate: ((sample_rate << 12) / 44100) as u16,
        length: length as u16,
    };
    let uploaded = upload(
        sound.offset,
        &data[VAG_HEADER_SIZE..],
        sound.length as usize,
        true,
    );

    SPU_ALLOC_PTR.set(sound.offset + uploaded as u32);
    sound
}

pub fn sound_play_on_channel(sound: &Sound, left: u16, right: u16, ch: usize) {
    if ch >= NUM_CHANNELS || sound.offset == 0 {
        return;
    }

    spu_ch_vol_l(ch).write(left);
    spu_ch_vol_r(ch).write(right);
    spu_ch_freq(ch).write(sound.sample_rate);
    spu_ch_addr(ch).write((sound.offset / 8) as u16);
    spu_ch_adsr1(ch).write(0x00ff);
    spu_ch_adsr2(ch).write(0x0000);

    if ch < 16 {
        SPU_FLAG_ON1.write(1 << ch);
    } else {
        SPU_FLAG_ON2.write(1 << (ch - 16));
    }
}
