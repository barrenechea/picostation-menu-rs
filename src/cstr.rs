// ps1-bare-metal - (C) 2023 spicyjpeg
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

//! C string handling on NUL-terminated byte buffers, matching the menu's libc.
//! The end of a slice counts as a NUL terminator.

fn byte_at(s: &[u8], index: usize) -> u8 {
    s.get(index).copied().unwrap_or(0)
}

/// The bytes of `s` before its first NUL.
pub fn until_nul(s: &[u8]) -> &[u8] {
    match s.iter().position(|&byte| byte == 0) {
        Some(length) => &s[..length],
        None => s,
    }
}

/// Compares bytes as signed chars, so bytes 0x80 and up sort before ASCII.
pub fn strcmp(lhs: &[u8], rhs: &[u8]) -> i32 {
    let mut index = 0;

    loop {
        let a = byte_at(lhs, index) as i8;
        let b = byte_at(rhs, index) as i8;

        if a != b {
            return a as i32 - b as i32;
        }
        if a == 0 {
            return 0;
        }
        index += 1;
    }
}

/// Unlike the standard function, this returns 0 as soon as either string ends,
/// so a string matches any string it is a prefix of.
pub fn strncmp(lhs: &[u8], rhs: &[u8], count: usize) -> i32 {
    for index in 0..count {
        let a = byte_at(lhs, index) as i8;
        let b = byte_at(rhs, index) as i8;

        if a == 0 || b == 0 {
            break;
        }
        if a != b {
            return a as i32 - b as i32;
        }
    }

    0
}
