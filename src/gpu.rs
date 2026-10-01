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

use crate::cell::compiler_barrier;
use crate::ps1::gpucmd::*;
use crate::ps1::registers::*;
use crate::psxproject::delay::delay_microseconds;
use crate::psxproject::irq::wait_for_vblank;

const DMA_MAX_CHUNK_SIZE: usize = 16;
const CHAIN_BUFFER_SIZE: usize = 16384;
const MAX_PACKET_SIZE: usize = 16;

/// A GPU display list, sent to the GPU by DMA as a linked list of packets.
pub struct DmaChain {
    data: [u32; CHAIN_BUFFER_SIZE],
    next_packet: usize,
    discarded: [u32; MAX_PACKET_SIZE],
}

#[derive(Clone, Copy)]
pub struct TextureInfo {
    pub u: u8,
    pub v: u8,
    pub width: u16,
    pub height: u16,
    pub page: u16,
    pub clut: u16,
}

impl DmaChain {
    pub const fn new() -> Self {
        Self {
            data: [0; CHAIN_BUFFER_SIZE],
            next_packet: 0,
            discarded: [0; MAX_PACKET_SIZE],
        }
    }

    pub fn reset(&mut self) {
        self.next_packet = 0;
    }

    /// Packets that don't fit in the chain (which keeps room for the end tag)
    /// are written to a scratch buffer and dropped from the frame.
    pub fn allocate_packet<const N: usize>(&mut self) -> &mut [u32; N] {
        let start = self.next_packet;
        let next = start + 1 + N;

        let packet = if next < CHAIN_BUFFER_SIZE {
            let next_address = self.data.as_ptr().wrapping_add(next).expose_provenance() as u32;

            self.next_packet = next;
            self.data[start] = gp0_tag(N, next_address);
            &mut self.data[start + 1..next]
        } else {
            &mut self.discarded[..N]
        };

        packet.try_into().unwrap()
    }

    pub fn end(&mut self) {
        self.data[self.next_packet] = gp0_end_tag(0);
    }
}

/// The mode the BIOS shows its "PS" logo and its shell in. A menu booted from
/// disc keeps it, so the TV, or any scaler in between, doesn't have to resync.
pub const SCREEN_WIDTH: i32 = 640;
pub const SCREEN_HEIGHT: i32 = 480;

pub const VRAM_WIDTH: i32 = 1024;
/// Textures go to the right of the framebuffer, within its first page row.
pub const TEXTURE_AREA_HEIGHT: i32 = 256;

/// The display range the BIOS sets for the shell on every version, and the
/// length of a field. The "PS" logo uses the same ranges, except on PAL v4.x,
/// which shows 15 more lines there.
struct Timing {
    range_h: (u32, u32),
    range_v: (u32, u32),
    field_lines: u32,
}

const NTSC_TIMING: Timing = Timing {
    range_h: (0x260, 0xc60),
    range_v: (0x010, 0x0ff),
    field_lines: 263,
};
const PAL_TIMING: Timing = Timing {
    range_h: (0x27e, 0xc7e),
    range_v: (0x02b, 0x11a),
    field_lines: 313,
};

/// Rounded up from the 63.6us of NTSC and the 64us of PAL.
const LINE_TIME: u32 = 64;
const VBLANK_MARGIN_LINES: u32 = 8;

/// Where GPUSTAT reports the display mode set by `fb_mode`.
const fn stat_fb_mode(fb_mode: u32) -> u32 {
    ((fb_mode & 0x3f) << 17) | (((fb_mode >> 6) & 1) << 16)
}

pub struct Display {
    /// From the vblank IRQ, at the end of the display range, to the start of
    /// the next field's.
    vblank_time: i32,
}

/// Sets up a single 640x480 interlaced framebuffer at the top left of VRAM,
/// with the display left blanked if it was.
///
/// Booted from disc, the BIOS has left that mode on, so the GPU isn't reset,
/// as that would switch to 256x240 progressive for a moment. Booted from a
/// cartridge before the BIOS intro, it's set up from scratch, with the video
/// standard the BIOS would have picked.
pub fn setup_gpu(bios_pal: bool) -> Display {
    let status = GPU_GP1.read();
    // Whatever is on screen already decides the video standard.
    let pal = if status & GP1_STAT_DISP_BLANK == 0 {
        status & GP1_STAT_FB_MODE_PAL != 0
    } else {
        bios_pal
    };
    let (mode, timing) = if pal {
        (Gp1VideoMode::Pal, &PAL_TIMING)
    } else {
        (Gp1VideoMode::Ntsc, &NTSC_TIMING)
    };
    let fb_mode = gp1_fb_mode(
        Gp1HorizontalRes::Res640,
        Gp1VerticalRes::Res480,
        mode,
        true,
        GP1_COLOR_16BPP,
    );

    if status & (GP1_STAT_FB_MODE_BITMASK | GP1_STAT_DISP_BLANK) == stat_fb_mode(fb_mode) {
        // Only clear what a reset would that the menu relies on.
        GPU_GP1.write(gp1_reset_fifo());
        wait_for_gp0_ready();
        GPU_GP0.write(gp0_tex_window_off());
        GPU_GP0.write(gp0_mask_off());
    } else {
        GPU_GP1.write(gp1_reset_gpu());
    }

    // The BIOS rewrites these every frame. The ranges don't affect sync, only
    // where the picture sits.
    let (x1, x2) = timing.range_h;
    let (y1, y2) = timing.range_v;

    GPU_GP1.write(gp1_fb_offset(0, 0));
    GPU_GP1.write(gp1_fb_range_h(x1, x2));
    GPU_GP1.write(gp1_fb_range_v(y1, y2));
    GPU_GP1.write(fb_mode);

    Display {
        vblank_time: ((timing.field_lines - y2 + y1 + VBLANK_MARGIN_LINES) * LINE_TIME) as i32,
    }
}

impl Display {
    /// Has the GPU draw a frame on the field that's off screen, which leaves
    /// the one on screen intact. Frames are drawn once the field is on screen,
    /// as it's undocumented which field the GPU skips during vblank: GPUSTAT
    /// reports the even one then. Frames drawn right at the vblank IRQ showed
    /// stale fields on hardware. As each frame only reaches one field, a frame
    /// that has to stay on screen is presented twice.
    pub fn present(&self, chain: &DmaChain) {
        wait_for_gp0_ready();
        wait_for_vblank();
        delay_microseconds(self.vblank_time);
        send_linked_list(chain);
    }
}

/// Fills a VRAM area right away, on both fields.
pub fn fill_vram(x: i32, y: i32, width: i32, height: i32, color: u32) {
    wait_for_gp0_ready();
    GPU_GP0.write(gp0_texpage(0, false, true));
    GPU_GP0.write(color | gp0_vram_fill());
    GPU_GP0.write(gp0_xy(x, y));
    GPU_GP0.write(gp0_xy(width, height));
}

pub fn wait_for_gp0_ready() {
    while GPU_GP1.read() & GP1_STAT_CMD_READY == 0 {}
}

pub fn wait_for_dma_done() {
    while dma_chcr(DMA_GPU).read() & DMA_CHCR_ENABLE != 0 {}
}

pub fn send_linked_list(chain: &DmaChain) {
    wait_for_dma_done();
    compiler_barrier();

    dma_madr(DMA_GPU).write(chain.data.as_ptr().expose_provenance() as u32);
    dma_chcr(DMA_GPU).write(DMA_CHCR_WRITE | DMA_CHCR_MODE_LIST | DMA_CHCR_ENABLE);
}

fn send_vram_data(data: &[u8], x: i32, y: i32, width: i32, height: i32) {
    wait_for_dma_done();

    let length = ((width * height) / 2) as usize;
    let (chunk_size, num_chunks) = if length < DMA_MAX_CHUNK_SIZE {
        (length, 1)
    } else {
        (DMA_MAX_CHUNK_SIZE, length / DMA_MAX_CHUNK_SIZE)
    };

    wait_for_gp0_ready();
    GPU_GP0.write(gp0_vram_write());
    GPU_GP0.write(gp0_xy(x, y));
    GPU_GP0.write(gp0_xy(width, height));

    compiler_barrier();
    dma_madr(DMA_GPU).write(data.as_ptr().expose_provenance() as u32);
    dma_bcr(DMA_GPU).write(chunk_size as u32 | ((num_chunks as u32) << 16));
    dma_chcr(DMA_GPU).write(DMA_CHCR_WRITE | DMA_CHCR_MODE_SLICE | DMA_CHCR_ENABLE);
}

#[allow(clippy::too_many_arguments)]
pub fn upload_indexed_texture(
    image: &[u8],
    palette: &[u8],
    x: i32,
    y: i32,
    palette_x: i32,
    palette_y: i32,
    width: i32,
    height: i32,
    color_depth: u16,
) -> TextureInfo {
    let (num_colors, width_divider) = if color_depth == GP0_COLOR_8BPP {
        (256, 2)
    } else {
        (16, 4)
    };

    send_vram_data(image, x, y, width / width_divider, height);
    wait_for_dma_done();
    send_vram_data(palette, palette_x, palette_y, num_colors, 1);
    wait_for_dma_done();

    TextureInfo {
        page: gp0_page(
            (x / 64) as u32,
            (y / 256) as u32,
            GP0_BLEND_SEMITRANS,
            color_depth,
        ),
        clut: gp0_clut((palette_x / 16) as u32, palette_y as u32),
        u: ((x % 64) * width_divider) as u8,
        v: (y % 256) as u8,
        width: width as u16,
        height: height as u16,
    }
}
