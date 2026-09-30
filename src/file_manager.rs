//! Storage and sorting for the directory listing sent by the PicoStation.

use crate::cstr::{strcmp, until_nul};

pub const MAX_FILE_LENGTH: usize = 255;
pub const MAX_FILE_ITEMS: usize = 4096;

#[derive(Clone, Copy)]
pub struct FileData {
    /// 1 for directories, 0 for files.
    pub flag: u8,
    filename: [u8; MAX_FILE_LENGTH + 1],
}

impl FileData {
    pub fn filename(&self) -> &[u8] {
        until_nul(&self.filename)
    }
}

/// Entries are sorted through `index`, which maps list positions to entries.
pub struct FileManager {
    index: [u16; MAX_FILE_ITEMS],
    data: [FileData; MAX_FILE_ITEMS],
}

impl FileManager {
    pub const fn new() -> Self {
        Self {
            index: [0; MAX_FILE_ITEMS],
            data: [FileData {
                flag: 0,
                filename: [0; MAX_FILE_LENGTH + 1],
            }; MAX_FILE_ITEMS],
        }
    }

    /// Directories come first, then entries are sorted by name.
    fn compare(&self, index_a: u16, index_b: u16) -> i32 {
        let a = &self.data[index_a as usize];
        let b = &self.data[index_b as usize];

        if a.flag == 1 && b.flag == 0 {
            return -1;
        }
        if a.flag == 0 && b.flag == 1 {
            return 1;
        }

        strcmp(&a.filename, &b.filename)
    }

    fn quicksort(&mut self, left: u16, right: u16) {
        if left >= right {
            return;
        }

        let pivot = self.index[(left as usize + right as usize) / 2];
        let mut i = left as i32;
        let mut j = right as i32;

        while i <= j {
            while self.compare(self.index[i as usize], pivot) < 0 {
                i += 1;
            }
            while self.compare(self.index[j as usize], pivot) > 0 {
                j -= 1;
            }
            if i <= j {
                self.index.swap(i as usize, j as usize);
                i += 1;
                j -= 1;
            }
        }

        if (left as i32) < j {
            self.quicksort(left, j as u16);
        }
        if i < right as i32 {
            self.quicksort(i as u16, right);
        }
    }

    /// Hides each .bin file that is directly followed, in the sorted list, by a
    /// .cue file with the same name. Returns the new entry count.
    pub fn clean_list(&mut self, count: u16) -> u16 {
        let count = count as usize;
        let mut kept = 0;

        for i in 0..count {
            let entry = self.index[i];

            if i + 1 < count {
                let bin_name = self.data[entry as usize].filename();
                let cue_name = self.data[self.index[i + 1] as usize].filename();
                let len = bin_name.len();

                if bin_name.ends_with(b".bin")
                    && cue_name.len() == len
                    && bin_name[..len - 4] == cue_name[..len - 4]
                    && cue_name.ends_with(b".cue")
                {
                    continue;
                }
            }

            self.index[kept] = entry;
            kept += 1;
        }

        kept as u16
    }

    pub fn init_file_data(&mut self, index: u16, flag: u8, filename: &[u8]) {
        let file = &mut self.data[index as usize];
        let length = filename.len().min(MAX_FILE_LENGTH);

        file.flag = flag;
        file.filename[..length].copy_from_slice(&filename[..length]);
        file.filename[length] = 0;
        self.index[index as usize] = index;
    }

    pub fn get_file_data(&self, index: u16) -> Option<&FileData> {
        let file_index = *self.index.get(index as usize)?;

        self.data.get(file_index as usize)
    }

    pub fn get_file_index(&self, index: u16) -> u16 {
        self.index[index as usize]
    }

    pub fn sort(&mut self, count: u16) {
        if count > 0 {
            self.quicksort(0, count - 1);
        }
    }
}
