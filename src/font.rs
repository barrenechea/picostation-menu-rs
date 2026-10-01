//! Text, drawn with the 8x15 ASCII font in the BIOS ROM. Every PS1 BIOS has the
//! same one at the same address, made for the 640x480 screens of its shell, so
//! it needs no setup and fits about 70 columns and 26 lines on screen.
//!
//! The glyphs get a drop shadow and are packed with the menu's icons into a
//! single texture page, with the palette of the menu's own font texture.

use core::ptr;

use crate::aligned::Aligned;
use crate::assets::{FONT_PALETTE, FONT_TEXTURE};
use crate::cell::StaticCell;
use crate::cstr::until_nul;
use crate::gpu::{DmaChain, SCREEN_WIDTH, TextureInfo, upload_indexed_texture};
use crate::ps1::gpucmd::*;

pub const CHAR_WIDTH: i32 = 8;
pub const LINE_HEIGHT: i32 = 16;

/// Characters 0x21 to 0x7f, a byte per row with the leftmost pixel in bit 7.
/// 0x5c and 0x7e are the yen sign and an overline, as in Shift JIS.
const BIOS_FONT: usize = 0xbfc7_f8de;
/// More punctuation, including the backslash and the tilde.
const BIOS_FONT_EXTRA: usize = 0xbfc7_fe6f;
const BIOS_EXTRA_BACKSLASH: usize = 0;
const BIOS_EXTRA_TILDE: usize = 12;
const BIOS_GLYPH_WIDTH: usize = 8;
const BIOS_GLYPH_HEIGHT: usize = 15;

/// Width of a 4bpp texture page.
const ATLAS_WIDTH: usize = 256;
const CELL_SIZE: usize = 16;
const ATLAS_COLUMNS: usize = ATLAS_WIDTH / CELL_SIZE;
/// Six rows of characters from the space up, then the icons.
const ATLAS_HEIGHT: usize = CELL_SIZE * 7;
const ATLAS_VRAM_X: i32 = 704;
const FIRST_CHAR: u8 = b' ';
const LAST_CHAR: u8 = b'~';
const ICON_CELL: usize = 96;

/// Entries of the menu font's palette.
const COLOR_TRANSPARENT: u8 = 0xa;
const COLOR_TEXT: u8 = 0xb;
/// Semi-transparent black.
const COLOR_SHADOW: u8 = 0xc;

/// The icons in the menu's font texture, as character codes 0x8f to 0x96.
pub const ICON_DISC: u8 = 0x8f;
pub const ICON_FOLDER: u8 = 0x92;
const LAST_ICON: u8 = 0x96;

const FONT_TEXTURE_WIDTH: usize = 96;

/// Position and size of each icon within the menu's font texture.
static ICONS: [[u8; 4]; 8] = [
    [0, 73, 10, 10],  // 0x8f: disc
    [12, 73, 10, 10], // 0x90: square button
    [24, 73, 10, 10], // 0x91: X button
    [36, 73, 10, 9],  // 0x92: folder
    [48, 73, 10, 9],  // 0x93: folder with arrow
    [60, 73, 10, 10], // 0x94: page
    [72, 73, 10, 10], // 0x95: crossed circle
    [85, 73, 8, 8],   // 0x96: start button
];

static ATLAS: StaticCell<Aligned<[u8; ATLAS_WIDTH * ATLAS_HEIGHT / 2]>> =
    StaticCell::new(Aligned([0; ATLAS_WIDTH * ATLAS_HEIGHT / 2]));

fn set_pixel(atlas: &mut [u8], x: usize, y: usize, color: u8) {
    let byte = &mut atlas[(y * ATLAS_WIDTH + x) / 2];

    *byte = if x & 1 != 0 {
        (*byte & 0x0f) | (color << 4)
    } else {
        (*byte & 0xf0) | color
    };
}

fn cell_origin(cell: usize) -> (usize, usize) {
    (
        (cell % ATLAS_COLUMNS) * CELL_SIZE,
        (cell / ATLAS_COLUMNS) * CELL_SIZE,
    )
}

fn bios_glyph(ch: u8) -> usize {
    match ch {
        b'\\' => BIOS_FONT_EXTRA + BIOS_EXTRA_BACKSLASH * BIOS_GLYPH_HEIGHT,
        b'~' => BIOS_FONT_EXTRA + BIOS_EXTRA_TILDE * BIOS_GLYPH_HEIGHT,
        _ => BIOS_FONT + (ch - b'!') as usize * BIOS_GLYPH_HEIGHT,
    }
}

fn copy_bios_glyph(atlas: &mut [u8], ch: u8) {
    let glyph = ptr::without_provenance::<u8>(bios_glyph(ch));
    let (x, y) = cell_origin((ch - FIRST_CHAR) as usize);
    let mut rows = [0u8; BIOS_GLYPH_HEIGHT + 1];

    for (row, bits) in rows.iter_mut().take(BIOS_GLYPH_HEIGHT).enumerate() {
        *bits = unsafe { ptr::read_volatile(glyph.wrapping_add(row)) };
    }

    for row in 0..=BIOS_GLYPH_HEIGHT {
        let text = (rows[row] as u16) << 1;
        // The shadow is the glyph moved a pixel down and right.
        let shadow = if row > 0 { rows[row - 1] as u16 } else { 0 };

        for column in 0..=BIOS_GLYPH_WIDTH {
            let mask = 0x100 >> column;

            if text & mask != 0 {
                set_pixel(atlas, x + column, y + row, COLOR_TEXT);
            } else if shadow & mask != 0 {
                set_pixel(atlas, x + column, y + row, COLOR_SHADOW);
            }
        }
    }
}

fn copy_icon(atlas: &mut [u8], index: usize) {
    let [icon_x, icon_y, width, height] = ICONS[index].map(|value| value as usize);
    let (x, y) = cell_origin(ICON_CELL + index);
    let y = y + (CELL_SIZE - height) / 2;

    for row in 0..height {
        for column in 0..width {
            let offset = (icon_y + row) * FONT_TEXTURE_WIDTH + icon_x + column;
            let byte = FONT_TEXTURE.0[offset / 2];
            let color = if offset & 1 != 0 {
                byte >> 4
            } else {
                byte & 0x0f
            };

            set_pixel(atlas, x + column, y + row, color);
        }
    }
}

/// Where a character is in the atlas, how wide it's drawn and how far it
/// advances. Characters the font lacks are drawn as question marks.
fn glyph(ch: u8) -> (usize, i32, i32) {
    match ch {
        ICON_DISC..=LAST_ICON => {
            let index = (ch - ICON_DISC) as usize;

            (ICON_CELL + index, ICONS[index][2] as i32, CELL_SIZE as i32)
        }
        FIRST_CHAR..=LAST_CHAR => (
            (ch - FIRST_CHAR) as usize,
            BIOS_GLYPH_WIDTH as i32 + 1,
            CHAR_WIDTH,
        ),
        _ => glyph(b'?'),
    }
}

pub struct Font {
    texture: TextureInfo,
}

impl Font {
    /// Builds the atlas and uploads it to VRAM, to the right of the
    /// framebuffer.
    pub fn new() -> Self {
        let atlas = unsafe { &mut (*ATLAS.get()).0 };

        atlas.fill(COLOR_TRANSPARENT * 0x11);
        for ch in FIRST_CHAR + 1..=LAST_CHAR {
            copy_bios_glyph(atlas, ch);
        }
        for index in 0..ICONS.len() {
            copy_icon(atlas, index);
        }

        Self {
            texture: upload_indexed_texture(
                atlas,
                &FONT_PALETTE.0,
                ATLAS_VRAM_X,
                0,
                ATLAS_VRAM_X,
                ATLAS_HEIGHT as i32,
                ATLAS_WIDTH as i32,
                ATLAS_HEIGHT as i32,
                GP0_COLOR_4BPP,
            ),
        }
    }

    pub fn advance(ch: u8) -> i32 {
        glyph(ch).2
    }

    /// Selects the font's texture page, which `draw_char()` needs.
    pub fn select(&self, chain: &mut DmaChain) {
        chain.allocate_packet::<1>()[0] = gp0_texpage(self.texture.page, false, false);
    }

    /// Draws a character and returns how far it advances.
    pub fn draw_char(&self, chain: &mut DmaChain, x: i32, y: i32, ch: u8) -> i32 {
        let (cell, width, advance) = glyph(ch);

        if ch != b' ' {
            let (u, v) = cell_origin(cell);
            // Blending makes the semi-transparent shadow render correctly.
            let packet = chain.allocate_packet::<4>();

            packet[0] = gp0_rectangle(true, true, true);
            packet[1] = gp0_xy(x, y);
            packet[2] = gp0_uv(
                self.texture.u as u32 + u as u32,
                self.texture.v as u32 + v as u32,
                self.texture.clut,
            );
            packet[3] = gp0_xy(width, CELL_SIZE as i32);
        }

        advance
    }

    /// Prints a string, which may span several lines. Whatever runs past the
    /// right edge of the screen is cut off.
    pub fn print(&self, chain: &mut DmaChain, x: i32, y: i32, text: &[u8]) {
        let mut current_x = x;
        let mut current_y = y;

        self.select(chain);

        for &ch in until_nul(text) {
            if ch == b'\n' {
                current_x = x;
                current_y += LINE_HEIGHT;
            } else if current_x < SCREEN_WIDTH {
                current_x += self.draw_char(chain, current_x, current_y, ch);
            }
        }
    }
}
