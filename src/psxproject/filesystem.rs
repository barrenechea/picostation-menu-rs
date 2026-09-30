//! Minimal ISO 9660 support: looking up files in the root directory.

use crate::aligned::Aligned;
use crate::cell::StaticCell;
use crate::cstr::{strcmp, strncmp};

use super::cdrom::start_cdrom_read;

pub type Sector = Aligned<[u8; 2048]>;

static ROOT_DIR_DATA: StaticCell<Sector> = StaticCell::new(Aligned([0; 2048]));

struct DirectoryEntry<'a> {
    record_length: usize,
    lba: u32,
    name: &'a [u8],
}

/// Reads a little-endian 32-bit value (ISO 9660 stores both byte orders).
fn int32_lm(data: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ])
}

/// Returns `None` at the end of the directory list, including when a record
/// would run past the end of the sector.
fn parse_dir_record(data: &[u8]) -> Option<DirectoryEntry<'_>> {
    if data.len() < 34 {
        return None;
    }

    let record_length = data[0] as usize;
    let lba = int32_lm(data, 2);

    if record_length < 1 {
        return None;
    }

    let name: &[u8] = match data[33] {
        0x00 => b".",
        0x01 => b"..",
        _ => data.get(33..33 + data[32] as usize)?,
    };

    Some(DirectoryEntry {
        record_length,
        lba,
        name,
    })
}

pub fn init_filesystem() -> bool {
    let mut buffer: Sector = Aligned([0; 2048]);

    // Read the primary volume descriptor.
    unsafe { start_cdrom_read(16, buffer.0.as_mut_ptr(), 1, 2048, true, true) };

    if strncmp(&buffer.0[8..], b"PLAYSTATION", 11) != 0 {
        return false;
    }

    let root_dir_lba = int32_lm(&buffer.0, 158);

    unsafe {
        start_cdrom_read(
            root_dir_lba,
            ROOT_DIR_DATA.get().cast(),
            1,
            2048,
            true,
            true,
        )
    };
    true
}

/// Returns the LBA of a file in the root directory, or 0 if it isn't there.
pub fn get_lba_to_file(filename: &[u8]) -> u32 {
    let root_dir_data = unsafe { &(*ROOT_DIR_DATA.get()).0 };
    let mut offset = 0;

    while offset < root_dir_data.len() {
        let Some(entry) = parse_dir_record(&root_dir_data[offset..]) else {
            break;
        };

        offset += entry.record_length;

        if strcmp(entry.name, filename) == 0 {
            return entry.lba;
        }
    }

    0
}

/// Loads the first sector of a file in the root directory. Returns whether the
/// file was found.
pub fn file_load(name: &[u8], sector_buffer: &mut Sector) -> bool {
    let lba = get_lba_to_file(name);

    if lba == 0 {
        return false;
    }

    unsafe { start_cdrom_read(lba, sector_buffer.0.as_mut_ptr(), 1, 2048, true, true) };
    true
}
