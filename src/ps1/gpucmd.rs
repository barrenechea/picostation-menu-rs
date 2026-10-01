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

//! GP0 (drawing) and GP1 (display control) command words.

/* DMA tags */

pub const fn gp0_tag(length: usize, next: u32) -> u32 {
    (next & 0xffffff) | (((length as u32) & 0xff) << 24)
}

pub const fn gp0_end_tag(length: usize) -> u32 {
    gp0_tag(length, 0xffffff)
}

/* Drawing attributes */

pub const GP0_BLEND_SEMITRANS: u16 = 0;

pub const GP0_COLOR_4BPP: u16 = 0;
pub const GP0_COLOR_8BPP: u16 = 1;

pub const fn gp0_page(x: u32, y: u32, blend_mode: u16, color_depth: u16) -> u16 {
    ((x & 15)
        | ((y & 1) << 4)
        | ((blend_mode as u32 & 3) << 5)
        | ((color_depth as u32 & 3) << 7)
        | ((y & 2) << 10)) as u16
}

pub const fn gp0_clut(x: u32, y: u32) -> u16 {
    ((x & 0x03f) | ((y & 0x3ff) << 6)) as u16
}

pub const fn gp0_xy(x: i32, y: i32) -> u32 {
    (x as u32 & 0xffff) | ((y as u32 & 0xffff) << 16)
}

pub const fn gp0_uv(u: u32, v: u32, attr: u16) -> u32 {
    (u & 0x00ff) | ((v & 0x00ff) << 8) | ((attr as u32) << 16)
}

pub const fn gp0_rgb(r: u8, g: u8, b: u8) -> u32 {
    (r as u32) | ((g as u32) << 8) | ((b as u32) << 16)
}

/* GP0 (drawing) commands */

const GP0_CMD_MISC: u32 = 0 << 29;
const GP0_CMD_POLYGON: u32 = 1 << 29;
const GP0_CMD_RECTANGLE: u32 = 3 << 29;
const GP0_CMD_VRAM_WRITE: u32 = 5 << 29;
const GP0_CMD_ATTRIBUTE: u32 = 7 << 29;

const GP0_CMD_VRAM_FILL: u32 = GP0_CMD_MISC | (2 << 24);

const GP0_CMD_TEXPAGE: u32 = GP0_CMD_ATTRIBUTE | (1 << 24);
const GP0_CMD_TEX_WINDOW: u32 = GP0_CMD_ATTRIBUTE | (2 << 24);
const GP0_CMD_FB_OFFSET1: u32 = GP0_CMD_ATTRIBUTE | (3 << 24);
const GP0_CMD_FB_OFFSET2: u32 = GP0_CMD_ATTRIBUTE | (4 << 24);
const GP0_CMD_FB_ORIGIN: u32 = GP0_CMD_ATTRIBUTE | (5 << 24);
const GP0_CMD_FB_MASK: u32 = GP0_CMD_ATTRIBUTE | (6 << 24);

const fn gp0_polygon(
    quad: bool,
    unshaded: bool,
    gouraud: bool,
    textured: bool,
    blend: bool,
) -> u32 {
    GP0_CMD_POLYGON
        | ((unshaded as u32) << 24)
        | ((blend as u32) << 25)
        | ((textured as u32) << 26)
        | ((quad as u32) << 27)
        | ((gouraud as u32) << 28)
}

pub const fn gp0_quad(textured: bool, blend: bool) -> u32 {
    gp0_polygon(true, true, false, textured, blend)
}

pub const fn gp0_rectangle(textured: bool, unshaded: bool, blend: bool) -> u32 {
    GP0_CMD_RECTANGLE
        | ((unshaded as u32) << 24)
        | ((blend as u32) << 25)
        | ((textured as u32) << 26)
}

pub const fn gp0_vram_write() -> u32 {
    GP0_CMD_VRAM_WRITE
}

pub const fn gp0_vram_fill() -> u32 {
    GP0_CMD_VRAM_FILL
}

pub const fn gp0_texpage(page: u16, dither: bool, unlock_fb: bool) -> u32 {
    GP0_CMD_TEXPAGE | (page as u32 & 0x9ff) | ((dither as u32) << 9) | ((unlock_fb as u32) << 10)
}

pub const fn gp0_tex_window_off() -> u32 {
    GP0_CMD_TEX_WINDOW
}

pub const fn gp0_mask_off() -> u32 {
    GP0_CMD_FB_MASK
}

pub const fn gp0_fb_offset1(x: u32, y: u32) -> u32 {
    GP0_CMD_FB_OFFSET1 | (x & 0x3ff) | ((y & 0x3ff) << 10)
}

pub const fn gp0_fb_offset2(x: u32, y: u32) -> u32 {
    GP0_CMD_FB_OFFSET2 | (x & 0x3ff) | ((y & 0x3ff) << 10)
}

pub const fn gp0_fb_origin(x: i32, y: i32) -> u32 {
    GP0_CMD_FB_ORIGIN | (x as u32 & 0x7ff) | ((y as u32 & 0x7ff) << 11)
}

/* GP1 (display control) commands */

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Gp1HorizontalRes {
    Res640 = 3,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Gp1VerticalRes {
    /// Only takes effect in interlaced mode.
    Res480 = 1,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Gp1VideoMode {
    Ntsc = 0,
    Pal = 1,
}

pub const GP1_COLOR_16BPP: u32 = 0;

pub const GP1_DREQ_GP0_WRITE: u32 = 2;

const GP1_CMD_RESET_GPU: u32 = 0 << 24;
const GP1_CMD_RESET_FIFO: u32 = 1 << 24;
const GP1_CMD_DISP_BLANK: u32 = 3 << 24;
const GP1_CMD_DREQ_MODE: u32 = 4 << 24;
const GP1_CMD_FB_OFFSET: u32 = 5 << 24;
const GP1_CMD_FB_RANGE_H: u32 = 6 << 24;
const GP1_CMD_FB_RANGE_V: u32 = 7 << 24;
const GP1_CMD_FB_MODE: u32 = 8 << 24;

pub const fn gp1_reset_gpu() -> u32 {
    GP1_CMD_RESET_GPU
}

pub const fn gp1_reset_fifo() -> u32 {
    GP1_CMD_RESET_FIFO
}

pub const fn gp1_disp_blank(blank: bool) -> u32 {
    GP1_CMD_DISP_BLANK | (blank as u32)
}

pub const fn gp1_dma_request_mode(mode: u32) -> u32 {
    GP1_CMD_DREQ_MODE | (mode & 3)
}

pub const fn gp1_fb_offset(x: u32, y: u32) -> u32 {
    GP1_CMD_FB_OFFSET | (x & 0x3ff) | ((y & 0x3ff) << 10)
}

pub const fn gp1_fb_range_h(low: u32, high: u32) -> u32 {
    GP1_CMD_FB_RANGE_H | (low & 0xfff) | ((high & 0xfff) << 12)
}

pub const fn gp1_fb_range_v(low: u32, high: u32) -> u32 {
    GP1_CMD_FB_RANGE_V | (low & 0x3ff) | ((high & 0x3ff) << 10)
}

pub const fn gp1_fb_mode(
    horizontal_res: Gp1HorizontalRes,
    vertical_res: Gp1VerticalRes,
    video_mode: Gp1VideoMode,
    interlace: bool,
    color_depth: u32,
) -> u32 {
    GP1_CMD_FB_MODE
        | (horizontal_res as u32 & 0x47)
        | ((vertical_res as u32 & 1) << 2)
        | ((video_mode as u32 & 1) << 3)
        | ((color_depth & 1) << 4)
        | ((interlace as u32) << 5)
}
