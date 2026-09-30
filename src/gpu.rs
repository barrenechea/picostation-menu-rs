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

pub fn setup_gpu(mode: Gp1VideoMode, width: i32, height: i32) {
    let x = 0x760;
    let y = if mode == Gp1VideoMode::Pal {
        0xa3
    } else {
        0x88
    };

    let horizontal_res = Gp1HorizontalRes::Res320;
    let vertical_res = Gp1VerticalRes::Res256;

    let offset_x = (width * gp1_clock_multiplier_h(horizontal_res) as i32) / 2;
    let offset_y = (height / gp1_clock_divider_v(vertical_res) as i32) / 2;

    GPU_GP1.write(gp1_reset_gpu());
    GPU_GP1.write(gp1_fb_range_h((x - offset_x) as u32, (x + offset_x) as u32));
    GPU_GP1.write(gp1_fb_range_v((y - offset_y) as u32, (y + offset_y) as u32));
    GPU_GP1.write(gp1_fb_mode(
        horizontal_res,
        vertical_res,
        mode,
        false,
        GP1_COLOR_16BPP,
    ));
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
