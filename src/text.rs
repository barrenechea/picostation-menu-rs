use core::fmt::NumBuffer;

/// A fixed-size string builder standing in for `snprintf()`: like it, output
/// that doesn't fit (leaving room for the terminator) is dropped.
pub struct TextBuffer<const N: usize> {
    bytes: [u8; N],
    length: usize,
}

impl<const N: usize> TextBuffer<N> {
    pub const fn new() -> Self {
        Self {
            bytes: [0; N],
            length: 0,
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.length]
    }

    pub fn push(&mut self, byte: u8) {
        if self.length + 1 < N {
            self.bytes[self.length] = byte;
            self.length += 1;
        }
    }

    pub fn push_bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.push(byte);
        }
    }

    /// Equivalent to `%u`.
    pub fn push_decimal(&mut self, value: u32) {
        self.push_decimal_left(value, 0);
    }

    /// Equivalent to `%-{width}u`: left-aligned and padded with spaces.
    pub fn push_decimal_left(&mut self, value: u32, width: usize) {
        let mut buffer = NumBuffer::new();
        let digits = value.format_into(&mut buffer);

        self.push_bytes(digits.as_bytes());
        for _ in digits.len()..width {
            self.push(b' ');
        }
    }
}
