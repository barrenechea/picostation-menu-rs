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
mod font;
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
use font::{Font, ICON_DISC, ICON_FOLDER, LINE_HEIGHT};
use gpu::*;
use ps1::cdrom::{CDROM_CMD_TEST, CDROM_TEST_DSP_CMD};
use ps1::gpucmd::*;
use ps1::registers::*;
use psxproject::cdrom::{
    init_cdrom, is_playstation_cd, issue_cdrom_command, start_cdrom_read, update_cdrom_toc,
};
use psxproject::delay::delay_microseconds;
use psxproject::filesystem::{Sector, file_load, init_filesystem};
use psxproject::irq::init_irq;
use psxproject::spu::{init_spu, sound_load_sound_from_binary, sound_play_on_channel};
use psxproject::system::{bios_is_pal, soft_fast_reboot, soft_reset};
use text::TextBuffer;

/// Size of a directory listing returned by the PicoStation, which follows the
/// 12-byte header of a 2340-byte raw sector.
const LISTING_SIZE: usize = 2324;
const MAX_FILES: usize = MAX_FILE_ITEMS;

const SFX_VOL: u16 = 10922; // 2/3 of maximal volume

const LOGO_WIDTH: i32 = 128;
const LOGO_HEIGHT: i32 = 20;
/// A line clear of the logo, so its edge texels stay transparent.
const LOGO_PALETTE_Y: i32 = LOGO_HEIGHT + 1;
/// The logo was drawn for a 320x240 screen.
const LOGO_SCALE: i32 = 2;

// Layout, in pixels of the 640x480 screen. TVs crop about 16 lines off the
// top and the bottom.

const BACKGROUND_COLOR: u32 = gp0_rgb(64, 64, 64);
const MARGIN_X: i32 = 32;
const LOGO_Y: i32 = 20;
const COUNTER_Y: i32 = 32;
const MESSAGE_X: i32 = 80;
const LIST_Y: i32 = 76;
const FOOTER_X: i32 = 24;
const FOOTER_Y: i32 = 440;
const CREDITS_TITLE_Y: i32 = 96;
const CREDITS_SCROLL_Y: i32 = 240;

const PAGE_SIZE: u16 = 22;

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

fn send_command(command: u8, argument: u16) {
    let test = [
        CDROM_TEST_DSP_CMD,
        0xf0 | command,
        (argument >> 8) as u8,
        argument as u8,
    ];

    issue_cdrom_command(CDROM_CMD_TEST, &test);
}

/// Draws a texture stretched to the given size, which may be negative to
/// mirror it.
fn draw_scaled(chain: &mut DmaChain, texture: &TextureInfo, x: i32, y: i32, w: i32, h: i32) {
    let u = texture.u as u32;
    let v = texture.v as u32;
    let u2 = u + texture.width as u32;
    let v2 = v + texture.height as u32;
    let x2 = x + w;
    let y2 = y + h;

    chain.allocate_packet::<1>()[0] = gp0_texpage(texture.page, false, false);

    let packet = chain.allocate_packet::<9>();

    packet[0] = gp0_quad(true, true);
    packet[1] = gp0_xy(x, y);
    packet[2] = gp0_uv(u, v, texture.clut);
    packet[3] = gp0_xy(x2, y);
    packet[4] = gp0_uv(u2, v, texture.page);
    packet[5] = gp0_xy(x, y2);
    packet[6] = gp0_uv(u, v2, 0);
    packet[7] = gp0_xy(x2, y2);
    packet[8] = gp0_uv(u2, v2, 0);
}

fn draw_logo(chain: &mut DmaChain, logo: &TextureInfo) {
    let width = LOGO_WIDTH * LOGO_SCALE;

    draw_scaled(
        chain,
        logo,
        (SCREEN_WIDTH - width) / 2,
        LOGO_Y,
        width,
        LOGO_HEIGHT * LOGO_SCALE,
    );
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

        let width = (LOGO_WIDTH * LOGO_SCALE * SCROLL_SINE_TABLE[self.sine_offset] as i32) / 127;

        draw_scaled(
            chain,
            logo,
            (SCREEN_WIDTH - width) / 2,
            LOGO_Y,
            width,
            LOGO_HEIGHT * LOGO_SCALE,
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
    /// Pixels scrolled per frame, a divisor of the character width.
    const SPEED: i32 = 2;
    const AMPLITUDE: i32 = 40;

    fn print(&mut self, chain: &mut DmaChain, font: &Font, x: i32, y: i32, text: &[u8]) {
        let mut current_x = x;
        let mut scroll_index = 0;
        let mut offset = 0;

        font.select(chain);

        self.x_ofs += Self::SPEED;
        if self.x_ofs >= Font::advance(text[self.string_offset]) {
            self.string_offset = (self.string_offset + 1) % text.len();
            self.x_ofs = 0;
        }

        loop {
            let wave = SCROLL_SINE_TABLE[(scroll_index + self.sine_offset) % 64] as i32;
            let current_y = y + (wave * Self::AMPLITUDE) / 128;

            scroll_index = (scroll_index + 1) % 64;

            self.delay += 1;
            if self.delay == 50 {
                self.sine_offset = (self.sine_offset + 1) % 64;
                self.delay = 0;
            }

            let ch = text[(offset + self.string_offset) % text.len()];

            offset += 1;
            current_x += font.draw_char(chain, current_x - self.x_ofs, current_y, ch);

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

    let display = setup_gpu(bios_is_pal());

    DMA_DPCR.set_bits(dma_dpcr_ch_enable(DMA_GPU));

    GPU_GP1.write(gp1_dma_request_mode(GP1_DREQ_GP0_WRITE));
    // VRAM still holds the BIOS logo and its textures. The scaled logo can
    // sample a texel past its edges, so the textures sit in transparent black.
    fill_vram(0, 0, SCREEN_WIDTH, SCREEN_HEIGHT, BACKGROUND_COLOR);
    fill_vram(
        SCREEN_WIDTH,
        0,
        VRAM_WIDTH - SCREEN_WIDTH,
        TEXTURE_AREA_HEIGHT,
        0,
    );
    GPU_GP1.write(gp1_disp_blank(false));

    let font = Font::new();
    let logo = upload_indexed_texture(
        &LOGO_TEXTURE.0,
        &LOGO_PALETTE.0,
        SCREEN_WIDTH,
        0,
        SCREEN_WIDTH,
        LOGO_PALETTE_Y,
        LOGO_WIDTH,
        LOGO_HEIGHT,
        GP0_COLOR_4BPP,
    );

    let mut using_second_chain = false;
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
        // The chains alternate so one can be built while the GPU reads the
        // other.
        let chain = &mut chains[using_second_chain as usize];
        using_second_chain = !using_second_chain;

        chain.reset();

        let packet = chain.allocate_packet::<4>();
        packet[0] = gp0_texpage(0, true, false);
        packet[1] = gp0_fb_offset1(0, 0);
        packet[2] = gp0_fb_offset2((SCREEN_WIDTH - 1) as u32, (SCREEN_HEIGHT - 1) as u32);
        packet[3] = gp0_fb_origin(0, 0);

        let packet = chain.allocate_packet::<3>();
        packet[0] = BACKGROUND_COLOR | gp0_vram_fill();
        packet[1] = gp0_xy(0, 0);
        packet[2] = gp0_xy(SCREEN_WIDTH, SCREEN_HEIGHT);

        if !credits_menu {
            draw_logo(chain, &logo);
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
                font.print(chain, MESSAGE_X, LIST_Y, b"Please Wait Loading...");
            } else {
                let mut counter = TextBuffer::<32>::new();

                counter.push_decimal(selected_index as u32 + 1);
                counter.push_bytes(b" of ");
                counter.push_decimal(file_entry_count);
                font.print(chain, MARGIN_X, COUNTER_Y, counter.as_bytes());

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

                        let y = LIST_Y + i * LINE_HEIGHT;

                        if index == selected_index as u32 {
                            let color = highlight + 48;
                            let packet = chain.allocate_packet::<3>();

                            packet[0] =
                                gp0_rgb(color, color, color) | gp0_rectangle(false, false, false);
                            packet[1] = gp0_xy(0, y);
                            packet[2] = gp0_xy(SCREEN_WIDTH, LINE_HEIGHT);
                        }

                        let Some(file) = files.get_file_data(index as u16) else {
                            continue;
                        };
                        let mut line = TextBuffer::<300>::new();

                        line.push_decimal_left(index + 1, 4);
                        line.push(b' ');
                        line.push(if file.flag == 0 {
                            ICON_DISC
                        } else {
                            ICON_FOLDER
                        });
                        line.push(b' ');
                        line.push_bytes(file.filename());
                        font.print(chain, MARGIN_X, y, line.as_bytes());
                    }
                } else {
                    font.print(chain, MESSAGE_X, LIST_Y, b"Empty Folder");
                }

                font.print(
                    chain,
                    FOOTER_X,
                    FOOTER_Y,
                    b"\x91 Select / Fast Boot, \x96 Regular Boot, \x90 Parent Folder",
                );

                highlight = (highlight + 1) & 0x3f;
            }
        } else {
            font.print(
                chain,
                MESSAGE_X,
                CREDITS_TITLE_Y,
                b"PicosStation/Plus Menu Alpha Release",
            );
            credits_scroll.print(chain, &font, 0, CREDITS_SCROLL_Y, CREDITS);
        }

        previous_buttons = buttons;
        chain.end();
        display.present(chain);

        // The command may keep the menu from drawing for a while.
        if current_command != MenuCommand::None {
            display.present(chain);
            wait_for_dma_done();
            wait_for_gp0_ready();
        }

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
