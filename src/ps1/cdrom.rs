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

//! CD-ROM drive data types, commands and status definitions.

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CdromMsf {
    pub minute: u8,
    pub second: u8,
    pub frame: u8,
}

impl CdromMsf {
    pub const fn as_bytes(&self) -> [u8; 3] {
        [self.minute, self.second, self.frame]
    }
}

pub const fn cdrom_encode_bcd(value: u8) -> u8 {
    value + (value / 10) * 6
}

pub const fn cdrom_convert_lba_to_msf(lba: u32) -> CdromMsf {
    // Skip the lead-in area (LBA 0 is always at 00:02:00)
    let lba = lba + 150;

    CdromMsf {
        minute: cdrom_encode_bcd((lba / (75 * 60)) as u8),
        second: cdrom_encode_bcd(((lba / 75) % 60) as u8),
        frame: cdrom_encode_bcd((lba % 75) as u8),
    }
}

pub const CDROM_CMD_SETLOC: u8 = 0x02;
pub const CDROM_CMD_READ_N: u8 = 0x06;
pub const CDROM_CMD_PAUSE: u8 = 0x09;
pub const CDROM_CMD_SETMODE: u8 = 0x0e;
pub const CDROM_CMD_SETSESSION: u8 = 0x12;
pub const CDROM_CMD_TEST: u8 = 0x19;

pub const CDROM_TEST_DSP_CMD: u8 = 0x50;

pub const CDROM_IRQ_DATA_READY: u8 = 1;
pub const CDROM_IRQ_COMPLETE: u8 = 2;
pub const CDROM_IRQ_ACKNOWLEDGE: u8 = 3;
pub const CDROM_IRQ_DATA_END: u8 = 4;
pub const CDROM_IRQ_ERROR: u8 = 5;

pub const CDROM_MODE_SIZE_2340: u8 = 2 << 4;
pub const CDROM_MODE_SPEED_2X: u8 = 1 << 7;
