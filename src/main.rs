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

//! PicoStation menu: lists the images on the PicoStation's SD card, mounts the
//! selected one and reboots into it.

#![no_std]
#![no_main]
#![feature(asm_experimental_arch)]

mod aligned;
mod assets;
mod cell;
mod controller;
mod cstr;
mod file_manager;
mod gpu;
mod ps1;
mod psxproject;
mod rt;
mod text;

use aligned::Aligned;
use assets::*;
use cell::StaticCell;
use controller::*;
use cstr::{strncmp, until_nul};
use file_manager::{FileManager, MAX_FILE_ITEMS};
use gpu::*;
use ps1::cdrom::{CDROM_CMD_TEST, CDROM_TEST_DSP_CMD};
use ps1::gpucmd::*;
use ps1::registers::*;
use psxproject::cdrom::{
    init_cdrom, is_playstation_cd, issue_cdrom_command, start_cdrom_read, update_cdrom_toc,
};
use psxproject::delay::delay_microseconds;
use psxproject::filesystem::{Sector, file_load, init_filesystem};
use psxproject::irq::{init_irq, wait_for_vblank};
use psxproject::spu::{init_spu, sound_load_sound_from_binary, sound_play_on_channel};
use psxproject::system::{soft_fast_reboot, soft_reset};
use text::TextBuffer;

/// Size of a directory listing returned by the PicoStation, which follows the
/// 12-byte header of a 2340-byte raw sector.
const LISTING_SIZE: usize = 2324;
const MAX_FILES: usize = MAX_FILE_ITEMS;

const SFX_VOL: u16 = 10922; // 2/3 of maximal volume

const SCREEN_WIDTH: i32 = 320;
const SCREEN_HEIGHT: i32 = 240;
const FONT_WIDTH: i32 = 96;
const FONT_HEIGHT: i32 = 84;
const TEXTURE_WIDTH: i32 = 128;
const TEXTURE_HEIGHT: i32 = 20;

const FONT_FIRST_TABLE_CHAR: u8 = b'!';
const FONT_INVALID_CHAR: u8 = 0x7f;
const FONT_SPACE_WIDTH: i32 = 4;
const FONT_TAB_WIDTH: i32 = 32;
const FONT_LINE_HEIGHT: i32 = 10;

const PAGE_SIZE: u16 = 16;

/* Commands understood by the PicoStation firmware */

const COMMAND_GOTO_ROOT: u8 = 0x1;
const COMMAND_GOTO_PARENT: u8 = 0x2;
const COMMAND_GOTO_DIRECTORY: u8 = 0x3;
const COMMAND_GET_NEXT_CONTENTS: u8 = 0x4;
const COMMAND_MOUNT_FILE: u8 = 0x5;
const COMMAND_BOOTLOADER: u8 = 0xa;

#[derive(Clone, Copy, PartialEq, Eq)]
enum MenuCommand {
    None,
    GotoRoot,
    GotoParent,
    GotoDirectory,
    MountFileFast,
    MountFileSlow,
    Bootloader,
}

/// Position and size of a character within the font spritesheet.
#[derive(Clone, Copy)]
struct SpriteInfo {
    x: u8,
    y: u8,
    width: u8,
    height: u8,
}

const fn sprite(x: u8, y: u8, width: u8, height: u8) -> SpriteInfo {
    SpriteInfo {
        x,
        y,
        width,
        height,
    }
}

/// Characters from '!' onwards, in ASCII order, followed by the "invalid
/// character" box at 0x7f and the button icons.
static FONT_SPRITES: [SpriteInfo; 118] = [
    sprite(6, 0, 2, 9),     // !
    sprite(12, 0, 4, 9),    // "
    sprite(18, 0, 6, 9),    // #
    sprite(24, 0, 6, 9),    // $
    sprite(30, 0, 6, 9),    // %
    sprite(36, 0, 6, 9),    // &
    sprite(42, 0, 2, 9),    // '
    sprite(48, 0, 3, 9),    // (
    sprite(54, 0, 3, 9),    // )
    sprite(60, 0, 4, 9),    // *
    sprite(66, 0, 6, 9),    // +
    sprite(72, 0, 3, 9),    // ,
    sprite(78, 0, 6, 9),    // -
    sprite(84, 0, 2, 9),    // .
    sprite(90, 0, 6, 9),    // /
    sprite(0, 9, 6, 9),     // 0
    sprite(6, 9, 6, 9),     // 1
    sprite(12, 9, 6, 9),    // 2
    sprite(18, 9, 6, 9),    // 3
    sprite(24, 9, 6, 9),    // 4
    sprite(30, 9, 6, 9),    // 5
    sprite(36, 9, 6, 9),    // 6
    sprite(42, 9, 6, 9),    // 7
    sprite(48, 9, 6, 9),    // 8
    sprite(54, 9, 6, 9),    // 9
    sprite(60, 9, 2, 9),    // :
    sprite(66, 9, 3, 9),    // ;
    sprite(72, 9, 6, 9),    // <
    sprite(78, 9, 6, 9),    // =
    sprite(84, 9, 6, 9),    // >
    sprite(90, 9, 6, 9),    // ?
    sprite(0, 18, 6, 9),    // @
    sprite(6, 18, 6, 9),    // A
    sprite(12, 18, 6, 9),   // B
    sprite(18, 18, 6, 9),   // C
    sprite(24, 18, 6, 9),   // D
    sprite(30, 18, 6, 9),   // E
    sprite(36, 18, 6, 9),   // F
    sprite(42, 18, 6, 9),   // G
    sprite(48, 18, 6, 9),   // H
    sprite(54, 18, 4, 9),   // I
    sprite(60, 18, 5, 9),   // J
    sprite(66, 18, 6, 9),   // K
    sprite(72, 18, 6, 9),   // L
    sprite(78, 18, 6, 9),   // M
    sprite(84, 18, 6, 9),   // N
    sprite(90, 18, 6, 9),   // O
    sprite(0, 27, 6, 9),    // P
    sprite(6, 27, 6, 9),    // Q
    sprite(12, 27, 6, 9),   // R
    sprite(18, 27, 6, 9),   // S
    sprite(24, 27, 6, 9),   // T
    sprite(30, 27, 6, 9),   // U
    sprite(36, 27, 6, 9),   // V
    sprite(42, 27, 6, 9),   // W
    sprite(48, 27, 6, 9),   // X
    sprite(54, 27, 6, 9),   // Y
    sprite(60, 27, 6, 9),   // Z
    sprite(66, 27, 3, 9),   // [
    sprite(72, 27, 6, 9),   // Backslash
    sprite(78, 27, 3, 9),   // ]
    sprite(84, 27, 4, 9),   // ^
    sprite(90, 27, 6, 9),   // _
    sprite(0, 36, 3, 9),    // `
    sprite(6, 36, 6, 9),    // a
    sprite(12, 36, 6, 9),   // b
    sprite(18, 36, 6, 9),   // c
    sprite(24, 36, 6, 9),   // d
    sprite(30, 36, 6, 9),   // e
    sprite(36, 36, 5, 9),   // f
    sprite(42, 36, 6, 9),   // g
    sprite(48, 36, 5, 9),   // h
    sprite(54, 36, 2, 9),   // i
    sprite(60, 36, 4, 9),   // j
    sprite(66, 36, 5, 9),   // k
    sprite(72, 36, 2, 9),   // l
    sprite(78, 36, 6, 9),   // m
    sprite(84, 36, 5, 9),   // n
    sprite(90, 36, 6, 9),   // o
    sprite(0, 45, 6, 9),    // p
    sprite(6, 45, 6, 9),    // q
    sprite(12, 45, 6, 9),   // r
    sprite(18, 45, 6, 9),   // s
    sprite(24, 45, 5, 9),   // t
    sprite(30, 45, 5, 9),   // u
    sprite(36, 45, 6, 9),   // v
    sprite(42, 45, 6, 9),   // w
    sprite(48, 45, 6, 9),   // x
    sprite(54, 45, 6, 9),   // y
    sprite(60, 45, 5, 9),   // z
    sprite(66, 45, 4, 9),   // {
    sprite(72, 45, 2, 9),   // |
    sprite(78, 45, 4, 9),   // }
    sprite(84, 45, 6, 9),   // ~
    sprite(90, 45, 6, 9),   // Invalid character
    sprite(0, 54, 6, 9),    // 0x80
    sprite(6, 54, 6, 9),    // 0x81
    sprite(12, 54, 4, 9),   // 0x82
    sprite(18, 54, 4, 9),   // 0x83
    sprite(24, 54, 6, 9),   // 0x84
    sprite(30, 54, 6, 9),   // 0x85
    sprite(36, 54, 6, 9),   // 0x86
    sprite(42, 54, 6, 9),   // 0x87
    sprite(0, 63, 7, 9),    // 0x88
    sprite(12, 63, 7, 9),   // 0x89
    sprite(24, 63, 9, 9),   // 0x8a
    sprite(36, 63, 8, 10),  // 0x8b
    sprite(48, 63, 11, 10), // 0x8c
    sprite(60, 63, 12, 10), // 0x8d
    sprite(72, 63, 14, 9),  // 0x8e
    sprite(0, 73, 10, 10),  // 0x8f: file icon
    sprite(12, 73, 10, 10), // 0x90: square button
    sprite(24, 73, 10, 10), // 0x91: X button
    sprite(36, 73, 10, 9),  // 0x92: folder icon
    sprite(48, 73, 10, 9),  // 0x93
    sprite(60, 73, 10, 10), // 0x94
    sprite(72, 73, 10, 10), // 0x95
    sprite(85, 73, 8, 8),   // 0x96: start button
];

static SCROLL_SINE_TABLE: [i8; 64] = [
    0, 12, 25, 37, 49, 60, 71, 81, //
    90, 98, 106, 112, 117, 122, 125, 126, //
    127, 126, 125, 122, 117, 112, 106, 98, //
    90, 81, 71, 60, 49, 37, 25, 12, //
    0, -12, -25, -37, -49, -60, -71, -81, //
    -90, -98, -106, -112, -117, -122, -125, -126, //
    -127, -126, -125, -122, -117, -112, -106, -98, //
    -90, -81, -71, -60, -49, -37, -25, -12, //
];

const CREDITS: &[u8] = b"Well here it is the PicosStation/Plus Credits.... Huge thanks go to Rama, Megavolt, Skitchin, SpicyJpeg, Danhans42, NicholasNoble and ChatGPT... Shout outs go out to every one on the PSX Dev & Xbox-Scene Discord... Until next time......................................................................................................";

static DMA_CHAINS: StaticCell<[DmaChain; 2]> = StaticCell::new([const { DmaChain::new() }; 2]);
static FILES: StaticCell<FileManager> = StaticCell::new(FileManager::new());

fn font_sprite(ch: u8) -> &'static SpriteInfo {
    let index = (ch as usize).wrapping_sub(FONT_FIRST_TABLE_CHAR as usize);

    FONT_SPRITES
        .get(index)
        .unwrap_or(&FONT_SPRITES[(FONT_INVALID_CHAR - FONT_FIRST_TABLE_CHAR) as usize])
}

fn send_command(command: u8, argument: u16) {
    let test = [
        CDROM_TEST_DSP_CMD,
        0xf0 | command,
        (argument >> 8) as u8,
        argument as u8,
    ];

    issue_cdrom_command(CDROM_CMD_TEST, &test);
}

fn draw_char(chain: &mut DmaChain, font: &TextureInfo, x: i32, y: i32, sprite: &SpriteInfo) {
    // Blending makes semitransparent pixels in the font render correctly.
    let packet = chain.allocate_packet::<4>();

    packet[0] = gp0_rectangle(true, true, true);
    packet[1] = gp0_xy(x, y);
    packet[2] = gp0_uv(
        font.u as u32 + sprite.x as u32,
        font.v as u32 + sprite.y as u32,
        font.clut,
    );
    packet[3] = gp0_xy(sprite.width as i32, sprite.height as i32);
}

fn print_string(chain: &mut DmaChain, font: &TextureInfo, x: i32, y: i32, text: &[u8]) {
    let mut current_x = x;
    let mut current_y = y;

    chain.allocate_packet::<1>()[0] = gp0_texpage(font.page, false, false);

    for &byte in until_nul(text) {
        let ch = match byte {
            b'\t' => {
                current_x += FONT_TAB_WIDTH - 1;
                current_x -= current_x % FONT_TAB_WIDTH;
                continue;
            }
            b'\n' => {
                current_x = x;
                current_y += FONT_LINE_HEIGHT;
                continue;
            }
            b' ' => {
                current_x += FONT_SPACE_WIDTH;
                continue;
            }
            0x99.. => FONT_INVALID_CHAR,
            _ => byte,
        };
        let sprite = font_sprite(ch);

        draw_char(chain, font, current_x, current_y, sprite);
        current_x += sprite.width as i32;
    }
}

/// Draws a sprite stretched to the given size, which may be negative to
/// mirror it.
#[allow(clippy::too_many_arguments)]
fn draw_scaled(
    chain: &mut DmaChain,
    texture: &TextureInfo,
    sprite: &SpriteInfo,
    x: i32,
    y: i32,
    w: i32,
    h: i32,
    blend: bool,
) {
    let u = sprite.x as u32;
    let v = sprite.y as u32;
    let u2 = u + sprite.width as u32;
    let v2 = v + sprite.height as u32;
    let x2 = x + w;
    let y2 = y + h;

    chain.allocate_packet::<1>()[0] = gp0_texpage(texture.page, false, false);

    let packet = chain.allocate_packet::<9>();

    packet[0] = gp0_quad(true, blend);
    packet[1] = gp0_xy(x, y);
    packet[2] = gp0_uv(u, v, texture.clut);
    packet[3] = gp0_xy(x2, y);
    packet[4] = gp0_uv(u2, v, texture.page);
    packet[5] = gp0_xy(x, y2);
    packet[6] = gp0_uv(u, v2, 0);
    packet[7] = gp0_xy(x2, y2);
    packet[8] = gp0_uv(u2, v2, 0);
}

/// The logo spinning around its vertical axis on the credits screen.
#[derive(Default)]
struct LogoSpin {
    delay: i32,
    sine_offset: usize,
}

impl LogoSpin {
    fn draw(&mut self, chain: &mut DmaChain, logo: &TextureInfo) {
        self.delay += 1;
        if self.delay == 2 {
            self.sine_offset = (self.sine_offset + 1) % 64;
            self.delay = 0;
        }

        let logo_width = (logo.width as i32 * SCROLL_SINE_TABLE[self.sine_offset] as i32) / 127;
        let sprite = SpriteInfo {
            x: logo.u,
            y: logo.v,
            width: logo.width as u8,
            height: logo.height as u8,
        };

        draw_scaled(
            chain,
            logo,
            &sprite,
            (SCREEN_WIDTH - logo_width) / 2,
            10,
            logo_width,
            logo.height as i32,
            true,
        );
    }
}

/// A line of text scrolling horizontally along a sine wave.
#[derive(Default)]
struct Scroller {
    delay: i32,
    string_offset: usize,
    sine_offset: usize,
    x_ofs: i32,
}

impl Scroller {
    fn char_width(ch: u8) -> i32 {
        if ch == b' ' {
            FONT_SPACE_WIDTH
        } else {
            font_sprite(ch).width as i32
        }
    }

    fn print(&mut self, chain: &mut DmaChain, font: &TextureInfo, x: i32, y: i32, text: &[u8]) {
        let mut current_x = x;
        let mut scroll_index = 0;
        let mut offset = 0;

        chain.allocate_packet::<1>()[0] = gp0_texpage(font.page, false, false);

        self.x_ofs += 1;
        if self.x_ofs >= Self::char_width(text[self.string_offset]) {
            self.string_offset = (self.string_offset + 1) % text.len();
            self.x_ofs = 0;
        }

        loop {
            let current_y =
                y + (SCROLL_SINE_TABLE[(scroll_index + self.sine_offset) % 64] as i32 * 20) / 128;

            scroll_index = (scroll_index + 1) % 64;

            self.delay += 1;
            if self.delay == 50 {
                self.sine_offset = (self.sine_offset + 1) % 64;
                self.delay = 0;
            }

            let ch = text[(offset + self.string_offset) % text.len()];

            offset += 1;

            if ch == b' ' {
                current_x += FONT_SPACE_WIDTH;
                continue;
            }

            let sprite = font_sprite(ch);

            draw_char(chain, font, current_x - self.x_ofs, current_y, sprite);
            current_x += sprite.width as i32;

            if current_x - self.x_ofs > SCREEN_WIDTH {
                break;
            }
        }
    }
}

/// Parses a listing sector into `files`. Each entry is a length byte, a flag
/// byte and the name; a zero length ends the sector. Returns whether the
/// PicoStation has more entries to send.
fn do_lookup(files: &mut FileManager, item_count: &mut u16, listing: &[u8]) -> bool {
    let mut offset = 0;

    while offset < LISTING_SIZE && (*item_count as usize) < MAX_FILES {
        let length = listing[offset] as usize;

        if length == 0 {
            return listing[offset + 1] == 1
                || (listing[offset + 2] == 0 && listing[offset + 3] == 0);
        }

        let name = &listing[(offset + 2).min(listing.len())..];

        files.init_file_data(
            *item_count,
            listing[offset + 1],
            &name[..length.min(name.len())],
        );
        offset += length + 2;
        *item_count += 1;
    }

    false
}

fn list_load(
    files: &mut FileManager,
    sector_buffer: &mut Aligned<[u8; 2340]>,
    command: u8,
    argument: u16,
) -> u32 {
    let mut file_entry_count = 0;
    let mut command = command;
    let mut argument = argument;
    let mut has_next = true;

    while has_next {
        send_command(command, argument);
        unsafe { start_cdrom_read(100, sector_buffer.0.as_mut_ptr(), 1, 2340, true, true) };

        has_next = do_lookup(files, &mut file_entry_count, &sector_buffer.0[12..]);
        command = COMMAND_GET_NEXT_CONTENTS;
        argument = file_entry_count;
    }

    files.sort(file_entry_count);
    files.clean_list(file_entry_count) as u32
}

/// Sends the boot executable named on the first line of SYSTEM.CNF to the
/// memory cards, so they can switch to the game's virtual card.
fn send_game_id_from_system_cnf(mcp_present: u8) {
    let mut config: Sector = Aligned([0; 2048]);

    if !file_load(b"SYSTEM.CNF;1", &mut config) {
        return;
    }

    let config = &config.0;
    let mut line = [0u8; 500];
    let mut i = 0;
    let mut j = 0;

    while config[i] != 0 && config[i] != b'\n' && i < 499 {
        if config[i] != b' ' && config[i] != b'\t' {
            line[j] = config[i];
            j += 1;
        }
        i += 1;
    }

    let game_id = if strncmp(&line, b"BOOT=", 5) == 0 {
        &line[5..]
    } else {
        &line[..]
    };

    send_game_id(until_nul(game_id), mcp_present);
}

fn mount(file_index: u16, mut command: MenuCommand, mcp_present: u8) -> ! {
    send_command(COMMAND_MOUNT_FILE, file_index);
    delay_microseconds(400_000);
    update_cdrom_toc();
    delay_microseconds(400_000);

    if is_playstation_cd() {
        if mcp_present != 0 && init_filesystem() {
            send_game_id_from_system_cnf(mcp_present);
        }
    } else {
        // Audio CDs can only be played from the BIOS shell.
        command = MenuCommand::MountFileSlow;
    }

    if command == MenuCommand::MountFileFast {
        soft_fast_reboot()
    } else {
        soft_reset()
    }
}

fn main() -> ! {
    init_irq();
    init_controller_bus();
    init_cdrom();
    init_spu();

    let mcp_present = check_mcp_present();

    let sfx_click = sound_load_sound_from_binary(&CLICK_SFX.0);
    let sfx_slide = sound_load_sound_from_binary(&SLIDE_SFX.0);

    let files = unsafe { &mut *FILES.get() };
    let chains = unsafe { &mut *DMA_CHAINS.get() };

    let mut current_command = MenuCommand::GotoRoot;

    if GPU_GP1.read() & GP1_STAT_FB_MODE_BITMASK == GP1_STAT_FB_MODE_PAL {
        setup_gpu(Gp1VideoMode::Pal, SCREEN_WIDTH, SCREEN_HEIGHT);
    } else {
        setup_gpu(Gp1VideoMode::Ntsc, SCREEN_WIDTH, SCREEN_HEIGHT);
    }

    DMA_DPCR.set_bits(dma_dpcr_ch_enable(DMA_GPU));

    GPU_GP1.write(gp1_dma_request_mode(GP1_DREQ_GP0_WRITE));
    GPU_GP1.write(gp1_disp_blank(false));

    let font = upload_indexed_texture(
        &FONT_TEXTURE.0,
        &FONT_PALETTE.0,
        SCREEN_WIDTH * 2,
        0,
        SCREEN_WIDTH * 2,
        FONT_HEIGHT,
        FONT_WIDTH,
        FONT_HEIGHT,
        GP0_COLOR_4BPP,
    );
    let logo = upload_indexed_texture(
        &LOGO_TEXTURE.0,
        &LOGO_PALETTE.0,
        SCREEN_WIDTH * 2,
        FONT_WIDTH,
        SCREEN_WIDTH * 2,
        TEXTURE_HEIGHT + FONT_WIDTH * 2,
        TEXTURE_WIDTH,
        TEXTURE_HEIGHT,
        GP0_COLOR_4BPP,
    );

    let mut using_second_frame = false;
    let mut sector_buffer = Aligned([0u8; 2340]);

    let mut highlight: u8 = 0;
    let mut hold: u8 = 0;
    let mut file_entry_count: u32 = 0;
    let mut selected_index: u16 = 0;
    let mut credits_menu = false;
    let mut logo_spin = LogoSpin::default();
    let mut credits_scroll = Scroller::default();

    let mut previous_buttons = get_button_press(0);

    loop {
        let buffer_x = if using_second_frame { SCREEN_WIDTH } else { 0 };
        let buffer_y = 0;

        let chain = &mut chains[using_second_frame as usize];
        using_second_frame = !using_second_frame;

        GPU_GP1.write(gp1_fb_offset(buffer_x as u32, buffer_y as u32));

        chain.reset();

        let packet = chain.allocate_packet::<4>();
        packet[0] = gp0_texpage(0, true, false);
        packet[1] = gp0_fb_offset1(buffer_x as u32, buffer_y as u32);
        packet[2] = gp0_fb_offset2(
            (buffer_x + SCREEN_WIDTH - 1) as u32,
            (buffer_y + SCREEN_HEIGHT - 2) as u32,
        );
        packet[3] = gp0_fb_origin(buffer_x, buffer_y);

        let packet = chain.allocate_packet::<3>();
        packet[0] = gp0_rgb(64, 64, 64) | gp0_vram_fill();
        packet[1] = gp0_xy(buffer_x, buffer_y);
        packet[2] = gp0_xy(SCREEN_WIDTH, SCREEN_HEIGHT);

        if !credits_menu {
            let packet = chain.allocate_packet::<5>();
            packet[0] = gp0_texpage(logo.page, false, false);
            packet[1] = gp0_rectangle(true, true, true);
            packet[2] = gp0_xy(96, 10);
            packet[3] = gp0_uv(logo.u as u32, logo.v as u32, logo.clut);
            packet[4] = gp0_xy(logo.width as i32, logo.height as i32);
        } else {
            logo_spin.draw(chain, &logo);
        }

        let buttons = get_button_press(0);
        let mut pressed_buttons = !previous_buttons & buttons;

        // Holding up or down auto-repeats after 31 frames, then every 6.
        if buttons & BUTTON_MASK_UP != 0 && previous_buttons & BUTTON_MASK_UP != 0 {
            hold += 1;
            if hold > 30 {
                pressed_buttons ^= BUTTON_MASK_UP;
                hold = 25;
            }
        } else if buttons & BUTTON_MASK_DOWN != 0 && previous_buttons & BUTTON_MASK_DOWN != 0 {
            hold += 1;
            if hold > 30 {
                pressed_buttons ^= BUTTON_MASK_DOWN;
                hold = 25;
            }
        } else {
            hold = 0;
        }

        if pressed_buttons & BUTTON_MASK_SELECT != 0 {
            credits_menu = !credits_menu;
        }

        if !credits_menu {
            // With an empty list, up wraps the selection around to 65535.
            let last_index = file_entry_count.wrapping_sub(1);

            if pressed_buttons & BUTTON_MASK_UP != 0 {
                selected_index = if selected_index > 0 {
                    selected_index - 1
                } else {
                    last_index as u16
                };
            } else if pressed_buttons & BUTTON_MASK_DOWN != 0 {
                selected_index = if (selected_index as i32) < last_index as i32 {
                    selected_index + 1
                } else {
                    0
                };
            }

            if pressed_buttons & (BUTTON_MASK_LEFT | BUTTON_MASK_L1) != 0 {
                selected_index = selected_index.saturating_sub(PAGE_SIZE);
            } else if pressed_buttons & (BUTTON_MASK_RIGHT | BUTTON_MASK_R1) != 0 {
                let last_page_start = file_entry_count.wrapping_sub(PAGE_SIZE as u32 + 1);

                selected_index = if (selected_index as i32) < last_page_start as i32 {
                    selected_index + PAGE_SIZE
                } else {
                    last_index as u16
                };
            }

            if pressed_buttons
                & (BUTTON_MASK_UP
                    | BUTTON_MASK_DOWN
                    | BUTTON_MASK_LEFT
                    | BUTTON_MASK_RIGHT
                    | BUTTON_MASK_L1
                    | BUTTON_MASK_R1)
                != 0
            {
                sound_play_on_channel(&sfx_click, SFX_VOL, SFX_VOL, 0);
            }

            let has_selection = (selected_index as u32) < file_entry_count;

            if pressed_buttons & BUTTON_MASK_START != 0
                && has_selection
                && let Some(file) = files.get_file_data(selected_index)
                && file.flag == 0
            {
                current_command = MenuCommand::MountFileSlow;
            }

            if pressed_buttons & BUTTON_MASK_X != 0
                && has_selection
                && let Some(file) = files.get_file_data(selected_index)
            {
                current_command = if file.flag == 0 {
                    MenuCommand::MountFileFast
                } else {
                    MenuCommand::GotoDirectory
                };
            }

            if pressed_buttons & BUTTON_MASK_SQUARE != 0 {
                current_command = MenuCommand::GotoParent;
            }

            if pressed_buttons & (BUTTON_MASK_SQUARE | BUTTON_MASK_X | BUTTON_MASK_START) != 0 {
                sound_play_on_channel(&sfx_slide, SFX_VOL, SFX_VOL, 1);
            }

            if pressed_buttons & BUTTON_MASK_TRIANGLE != 0 {
                current_command = MenuCommand::Bootloader;
            }

            if current_command != MenuCommand::None {
                print_string(chain, &font, 40, 40, b"Please Wait Loading...");
            } else {
                let mut counter = TextBuffer::<32>::new();

                counter.push_decimal(selected_index as u32 + 1);
                counter.push_bytes(b" of ");
                counter.push_decimal(file_entry_count);
                print_string(chain, &font, 16, 16, counter.as_bytes());

                let count = file_entry_count as i32;
                let page_size = PAGE_SIZE as i32;
                let mut start = 0;

                if count >= page_size {
                    start = (selected_index as i32 - page_size / 2)
                        .max(0)
                        .min(count - page_size);
                }

                let item_count = (start + page_size).min(count) - start;

                if item_count > 0 {
                    for i in 0..item_count {
                        let index = (start + i) as u32;

                        if index == selected_index as u32 {
                            let color = highlight + 48;
                            let packet = chain.allocate_packet::<3>();

                            packet[0] =
                                gp0_rgb(color, color, color) | gp0_rectangle(false, false, false);
                            packet[1] = gp0_xy(0, 32 + i * 11);
                            packet[2] = gp0_xy(SCREEN_WIDTH, 12);
                        }

                        let Some(file) = files.get_file_data(index as u16) else {
                            continue;
                        };
                        let mut line = TextBuffer::<300>::new();

                        line.push_decimal_left(index + 1, 4);
                        line.push(b' ');
                        line.push(if file.flag == 0 { 0x8f } else { 0x92 });
                        line.push(b' ');
                        line.push_bytes(file.filename());
                        line.push(b'\n');
                        print_string(chain, &font, 16, 34 + i * 11, line.as_bytes());
                    }
                } else {
                    print_string(chain, &font, 40, 40, b"Empty Folder");
                }

                print_string(
                    chain,
                    &font,
                    12,
                    212,
                    b"\x91 Select / Fast Boot, \x96 Regular Boot, \x90 Parent Folder",
                );

                highlight = (highlight + 1) & 0x3f;
            }
        } else {
            print_string(
                chain,
                &font,
                40,
                40,
                b"PicosStation/Plus Menu Alpha Release",
            );
            credits_scroll.print(chain, &font, 0, 120, CREDITS);
        }

        previous_buttons = buttons;
        chain.end();
        wait_for_gp0_ready();
        wait_for_vblank();
        send_linked_list(chain);

        match current_command {
            MenuCommand::None => {}
            MenuCommand::GotoRoot => {
                file_entry_count = list_load(files, &mut sector_buffer, COMMAND_GOTO_ROOT, 0);
            }
            MenuCommand::GotoParent => {
                file_entry_count = list_load(files, &mut sector_buffer, COMMAND_GOTO_PARENT, 0);
                selected_index = 0;
            }
            MenuCommand::Bootloader => {
                send_command(COMMAND_BOOTLOADER, 0xbeef);
            }
            MenuCommand::GotoDirectory => {
                let index = files.get_file_index(selected_index);

                file_entry_count =
                    list_load(files, &mut sector_buffer, COMMAND_GOTO_DIRECTORY, index);
                selected_index = 0;
            }
            MenuCommand::MountFileFast | MenuCommand::MountFileSlow => {
                mount(
                    files.get_file_index(selected_index),
                    current_command,
                    mcp_present,
                );
            }
        }

        current_command = MenuCommand::None;
    }
}
