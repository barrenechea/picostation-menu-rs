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

//! Memory-mapped hardware registers, accessed through the uncached KSEG1 mirror.

use core::marker::PhantomData;
use core::ops::{BitAnd, BitOr, Not};
use core::ptr::{self, read_volatile, write_volatile};

pub struct Reg<T> {
    addr: usize,
    _type: PhantomData<T>,
}

impl<T> Clone for Reg<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> Copy for Reg<T> {}

impl<T: Copy> Reg<T> {
    pub const fn at(addr: usize) -> Self {
        Self {
            addr,
            _type: PhantomData,
        }
    }

    #[inline(always)]
    pub fn read(self) -> T {
        unsafe { read_volatile(ptr::without_provenance(self.addr)) }
    }

    #[inline(always)]
    pub fn write(self, value: T) {
        unsafe { write_volatile(ptr::without_provenance_mut(self.addr), value) }
    }
}

impl<T: Copy + BitOr<Output = T> + BitAnd<Output = T> + Not<Output = T>> Reg<T> {
    #[inline(always)]
    pub fn set_bits(self, bits: T) {
        self.write(self.read() | bits);
    }

    #[inline(always)]
    pub fn clear_bits(self, bits: T) {
        self.write(self.read() & !bits);
    }
}

/* Constants */

pub const F_CPU: u32 = 33_868_800;

const IO_BASE: usize = 0xbf80_1000;

/* Bus interface */

pub const BIU_CTRL_RECOVERY: u32 = 1 << 8;
pub const BIU_CTRL_WIDTH_16: u32 = 1 << 12;
pub const BIU_CTRL_AUTO_INCR: u32 = 1 << 13;
pub const BIU_CTRL_DMA_DELAY: u32 = 1 << 29;

pub const BIU_DEV4_CTRL: Reg<u32> = Reg::at(IO_BASE | 0x014); // SPU
pub const BIU_DEV5_CTRL: Reg<u32> = Reg::at(IO_BASE | 0x018); // CD-ROM

/* Serial interface (controllers and memory cards) */

pub const SIO_STAT_TX_NOT_FULL: u16 = 1 << 0;
pub const SIO_STAT_RX_NOT_EMPTY: u16 = 1 << 1;

pub const SIO_MODE_BAUD_DIV1: u16 = 1 << 0;
pub const SIO_MODE_DATA_8: u16 = 3 << 2;

pub const SIO_CTRL_TX_ENABLE: u16 = 1 << 0;
pub const SIO_CTRL_DTR: u16 = 1 << 1;
pub const SIO_CTRL_RX_ENABLE: u16 = 1 << 2;
pub const SIO_CTRL_ACKNOWLEDGE: u16 = 1 << 4;
pub const SIO_CTRL_RESET: u16 = 1 << 6;
pub const SIO_CTRL_DSR_IRQ_ENABLE: u16 = 1 << 12;
pub const SIO_CTRL_CS_PORT_2: u16 = 1 << 13;

// SIO_DATA is a 32-bit register, but some emulators break if it's read more
// than 8 bits at a time.
pub const SIO_DATA0: Reg<u8> = Reg::at(IO_BASE | 0x040);
pub const SIO_STAT0: Reg<u16> = Reg::at(IO_BASE | 0x044);
pub const SIO_MODE0: Reg<u16> = Reg::at(IO_BASE | 0x048);
pub const SIO_CTRL0: Reg<u16> = Reg::at(IO_BASE | 0x04a);
pub const SIO_BAUD0: Reg<u16> = Reg::at(IO_BASE | 0x04e);

/* IRQ controller */

pub const IRQ_VSYNC: u32 = 0;
pub const IRQ_CDROM: u32 = 2;
pub const IRQ_SIO0: u32 = 7;
pub const IRQ_SPU: u32 = 9;

pub const IRQ_STAT: Reg<u16> = Reg::at(IO_BASE | 0x070);
pub const IRQ_MASK: Reg<u16> = Reg::at(IO_BASE | 0x074);

/* DMA */

pub const DMA_GPU: usize = 2;
pub const DMA_CDROM: usize = 3;
pub const DMA_SPU: usize = 4;

pub const DMA_CHCR_WRITE: u32 = 1 << 0;
pub const DMA_CHCR_MODE_SLICE: u32 = 1 << 9;
pub const DMA_CHCR_MODE_LIST: u32 = 2 << 9;
pub const DMA_CHCR_ENABLE: u32 = 1 << 24;
pub const DMA_CHCR_TRIGGER: u32 = 1 << 28;

pub const fn dma_dpcr_ch_enable(channel: usize) -> u32 {
    (1 << 3) << (4 * channel)
}

pub const DMA_DICR_IRQ_ENABLE: u32 = 1 << 23;
pub const DMA_DICR_CH_STAT_BITMASK: u32 = 0x7f << 24;

pub const fn dma_madr(channel: usize) -> Reg<u32> {
    Reg::at((IO_BASE | 0x080) + 16 * channel)
}

pub const fn dma_bcr(channel: usize) -> Reg<u32> {
    Reg::at((IO_BASE | 0x084) + 16 * channel)
}

pub const fn dma_chcr(channel: usize) -> Reg<u32> {
    Reg::at((IO_BASE | 0x088) + 16 * channel)
}

pub const DMA_DPCR: Reg<u32> = Reg::at(IO_BASE | 0x0f0);
pub const DMA_DICR: Reg<u32> = Reg::at(IO_BASE | 0x0f4);

/* CD-ROM drive */

pub const CDROM_HSTS_RSLRRDY: u8 = 1 << 5;
pub const CDROM_HSTS_BUSYSTS: u8 = 1 << 7;

pub const CDROM_HINT_INT0: u8 = 1 << 0;
pub const CDROM_HINT_INT1: u8 = 1 << 1;
pub const CDROM_HINT_INT2: u8 = 1 << 2;

pub const CDROM_HCHPCTL_BFRD: u8 = 1 << 7;

pub const CDROM_HCLRCTL_CLRINT0: u8 = 1 << 0;
pub const CDROM_HCLRCTL_CLRINT1: u8 = 1 << 1;
pub const CDROM_HCLRCTL_CLRINT2: u8 = 1 << 2;
pub const CDROM_HCLRCTL_CLRPRM: u8 = 1 << 6;

pub const CDROM_ADPCTL_CHNGATV: u8 = 1 << 5;

pub const CDROM_HSTS: Reg<u8> = Reg::at(IO_BASE | 0x800); // All banks
pub const CDROM_RESULT: Reg<u8> = Reg::at(IO_BASE | 0x801); // All banks
pub const CDROM_HINTSTS: Reg<u8> = Reg::at(IO_BASE | 0x803); // Bank 1

pub const CDROM_ADDRESS: Reg<u8> = Reg::at(IO_BASE | 0x800); // All banks
pub const CDROM_COMMAND: Reg<u8> = Reg::at(IO_BASE | 0x801); // Bank 0
pub const CDROM_PARAMETER: Reg<u8> = Reg::at(IO_BASE | 0x802); // Bank 0
pub const CDROM_HCHPCTL: Reg<u8> = Reg::at(IO_BASE | 0x803); // Bank 0
pub const CDROM_HINTMSK_W: Reg<u8> = Reg::at(IO_BASE | 0x802); // Bank 1
pub const CDROM_HCLRCTL: Reg<u8> = Reg::at(IO_BASE | 0x803); // Bank 1
pub const CDROM_ATV0: Reg<u8> = Reg::at(IO_BASE | 0x802); // Bank 2
pub const CDROM_ATV1: Reg<u8> = Reg::at(IO_BASE | 0x803); // Bank 2
pub const CDROM_ATV2: Reg<u8> = Reg::at(IO_BASE | 0x801); // Bank 3
pub const CDROM_ATV3: Reg<u8> = Reg::at(IO_BASE | 0x802); // Bank 3
pub const CDROM_ADPCTL: Reg<u8> = Reg::at(IO_BASE | 0x803); // Bank 3

/* GPU */

/// Bits 16-22 mirror the display mode set by GP1(08h), in a different order.
pub const GP1_STAT_FB_MODE_BITMASK: u32 = 0x7f << 16;
pub const GP1_STAT_FB_MODE_PAL: u32 = 1 << 20;
pub const GP1_STAT_DISP_BLANK: u32 = 1 << 23;
pub const GP1_STAT_CMD_READY: u32 = 1 << 26;

pub const GPU_GP0: Reg<u32> = Reg::at(IO_BASE | 0x810);
pub const GPU_GP1: Reg<u32> = Reg::at(IO_BASE | 0x814);

/* SPU */

pub const SPU_STAT_BUSY: u16 = 1 << 10;

pub const SPU_CTRL_XFER_BITMASK: u16 = 3 << 4;
pub const SPU_CTRL_XFER_WRITE: u16 = 1 << 4;
pub const SPU_CTRL_XFER_DMA_WRITE: u16 = 2 << 4;
pub const SPU_CTRL_UNMUTE: u16 = 1 << 14;
pub const SPU_CTRL_ENABLE: u16 = 1 << 15;

pub const fn spu_ch_vol_l(channel: usize) -> Reg<u16> {
    Reg::at((IO_BASE | 0xc00) + 16 * channel)
}

pub const fn spu_ch_vol_r(channel: usize) -> Reg<u16> {
    Reg::at((IO_BASE | 0xc02) + 16 * channel)
}

pub const fn spu_ch_freq(channel: usize) -> Reg<u16> {
    Reg::at((IO_BASE | 0xc04) + 16 * channel)
}

pub const fn spu_ch_addr(channel: usize) -> Reg<u16> {
    Reg::at((IO_BASE | 0xc06) + 16 * channel)
}

pub const fn spu_ch_adsr1(channel: usize) -> Reg<u16> {
    Reg::at((IO_BASE | 0xc08) + 16 * channel)
}

pub const fn spu_ch_adsr2(channel: usize) -> Reg<u16> {
    Reg::at((IO_BASE | 0xc0a) + 16 * channel)
}

pub const SPU_MASTER_VOL_L: Reg<u16> = Reg::at(IO_BASE | 0xd80);
pub const SPU_MASTER_VOL_R: Reg<u16> = Reg::at(IO_BASE | 0xd82);
pub const SPU_REVERB_VOL_L: Reg<u16> = Reg::at(IO_BASE | 0xd84);
pub const SPU_REVERB_VOL_R: Reg<u16> = Reg::at(IO_BASE | 0xd86);
pub const SPU_FLAG_ON1: Reg<u16> = Reg::at(IO_BASE | 0xd88);
pub const SPU_FLAG_ON2: Reg<u16> = Reg::at(IO_BASE | 0xd8a);
pub const SPU_FLAG_OFF1: Reg<u16> = Reg::at(IO_BASE | 0xd8c);
pub const SPU_FLAG_OFF2: Reg<u16> = Reg::at(IO_BASE | 0xd8e);
pub const SPU_FLAG_FM1: Reg<u16> = Reg::at(IO_BASE | 0xd90);
pub const SPU_FLAG_FM2: Reg<u16> = Reg::at(IO_BASE | 0xd92);
pub const SPU_FLAG_NOISE1: Reg<u16> = Reg::at(IO_BASE | 0xd94);
pub const SPU_FLAG_NOISE2: Reg<u16> = Reg::at(IO_BASE | 0xd96);
pub const SPU_FLAG_REVERB1: Reg<u16> = Reg::at(IO_BASE | 0xd98);
pub const SPU_FLAG_REVERB2: Reg<u16> = Reg::at(IO_BASE | 0xd9a);

pub const SPU_REVERB_ADDR: Reg<u16> = Reg::at(IO_BASE | 0xda2);
pub const SPU_ADDR: Reg<u16> = Reg::at(IO_BASE | 0xda6);
pub const SPU_DATA: Reg<u16> = Reg::at(IO_BASE | 0xda8);
pub const SPU_CTRL: Reg<u16> = Reg::at(IO_BASE | 0xdaa);
pub const SPU_DMA_CTRL: Reg<u16> = Reg::at(IO_BASE | 0xdac);
pub const SPU_STAT: Reg<u16> = Reg::at(IO_BASE | 0xdae);
