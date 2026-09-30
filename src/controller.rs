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

//! Controller and memory card access over the SIO0 serial bus, including the
//! game ID protocol spoken by memory card emulators such as the MemCard Pro.

use crate::ps1::registers::*;
use crate::psxproject::delay::delay_microseconds;

/// The bus is shared by all controllers and memory cards, so every packet
/// starts with the address of the device that shall respond to it.
#[derive(Clone, Copy)]
#[repr(u8)]
pub enum DeviceAddress {
    Controller = 0x01,
    MemoryCard = 0x81,
}

const CMD_POLL: u8 = b'B';
const CMD_CARD_IDENTIFY: u8 = b'S';
const CMD_GAME_ID_PING: u8 = b' ';
const CMD_GAME_ID_SEND: u8 = b'!';

pub const BUTTON_MASK_SELECT: u16 = 1 << 0;
pub const BUTTON_MASK_START: u16 = 1 << 3;
pub const BUTTON_MASK_UP: u16 = 1 << 4;
pub const BUTTON_MASK_RIGHT: u16 = 1 << 5;
pub const BUTTON_MASK_DOWN: u16 = 1 << 6;
pub const BUTTON_MASK_LEFT: u16 = 1 << 7;
pub const BUTTON_MASK_L1: u16 = 1 << 10;
pub const BUTTON_MASK_R1: u16 = 1 << 11;
pub const BUTTON_MASK_TRIANGLE: u16 = 1 << 12;
pub const BUTTON_MASK_X: u16 = 1 << 14;
pub const BUTTON_MASK_SQUARE: u16 = 1 << 15;

const DTR_DELAY: i32 = 150;
const DTR_PRE_DELAY: i32 = 10;
const DTR_POST_DELAY: i32 = 10;
const DSR_TIMEOUT: i32 = 120;
const BYTE_DELAY: i32 = 30;

/// The longest game ID that can be read from SYSTEM.CNF.
const MAX_GAME_ID_LENGTH: usize = 500;

pub fn init_controller_bus() {
    // 250000bps with 8 data bits, raising an IRQ whenever DSR is pulsed.
    SIO_CTRL0.write(SIO_CTRL_RESET);

    SIO_MODE0.write(SIO_MODE_BAUD_DIV1 | SIO_MODE_DATA_8);
    SIO_BAUD0.write((F_CPU / 250_000) as u16);
    SIO_CTRL0.write(SIO_CTRL_TX_ENABLE | SIO_CTRL_RX_ENABLE | SIO_CTRL_DSR_IRQ_ENABLE);
}

fn clear_acknowledge() {
    IRQ_STAT.write(!(1 << IRQ_SIO0));
    SIO_CTRL0.set_bits(SIO_CTRL_ACKNOWLEDGE);
}

/// Devices acknowledge each byte by pulsing DSR. Nothing is received if no
/// device is connected, hence the timeout.
fn wait_for_acknowledge(mut timeout: i32) -> bool {
    while timeout > 0 {
        if IRQ_STAT.read() & (1 << IRQ_SIO0) != 0 {
            clear_acknowledge();
            return true;
        }

        delay_microseconds(10);
        timeout -= 10;
    }

    false
}

/// Devices ignore packets unless DTR is asserted on their port.
fn select_port(port: usize) {
    if port != 0 {
        SIO_CTRL0.set_bits(SIO_CTRL_CS_PORT_2);
    } else {
        SIO_CTRL0.clear_bits(SIO_CTRL_CS_PORT_2);
    }
}

fn exchange_byte(value: u8) -> u8 {
    clear_acknowledge();
    while SIO_STAT0.read() & SIO_STAT_TX_NOT_FULL == 0 {}

    SIO_DATA0.write(value);

    while SIO_STAT0.read() & SIO_STAT_RX_NOT_EMPTY == 0 {}

    SIO_DATA0.read()
}

fn drain_rx_fifo() {
    while SIO_STAT0.read() & SIO_STAT_RX_NOT_EMPTY != 0 {
        SIO_DATA0.read();
    }
}

/// Sends `request` padded with zeroes while receiving up to `response.len()`
/// bytes. Returns the length of the response.
fn exchange_packet(address: DeviceAddress, request: &[u8], response: &mut [u8]) -> usize {
    delay_microseconds(DTR_PRE_DELAY);
    IRQ_STAT.write(!(1 << IRQ_SIO0));
    SIO_CTRL0.set_bits(SIO_CTRL_DTR | SIO_CTRL_ACKNOWLEDGE);
    delay_microseconds(DTR_DELAY);

    let mut resp_length = 0;

    SIO_DATA0.write(address as u8);

    if wait_for_acknowledge(DSR_TIMEOUT) {
        drain_rx_fifo();

        let mut request = request.iter().copied();

        while resp_length < response.len() {
            response[resp_length] = exchange_byte(request.next().unwrap_or(0));
            resp_length += 1;

            // Devices keep pulsing DSR as long as there is more data.
            if !wait_for_acknowledge(DSR_TIMEOUT) {
                break;
            }
        }
    }

    delay_microseconds(DTR_DELAY);
    SIO_CTRL0.clear_bits(SIO_CTRL_DTR);
    delay_microseconds(DTR_POST_DELAY);
    resp_length
}

/// Returns the pressed buttons as a bitfield of `BUTTON_MASK_*` values.
pub fn get_button_press(port: usize) -> u16 {
    let request = [CMD_POLL, 0x00, 0x00, 0x00];
    let mut response = [0u8; 32];

    select_port(port);
    let resp_length = exchange_packet(DeviceAddress::Controller, &request, &mut response);

    // All controllers reply with at least 4 bytes of data.
    if resp_length < 4 || response[1] != 0x5a {
        return 0;
    }

    // Buttons are active low.
    (response[2] as u16 | ((response[3] as u16) << 8)) ^ 0xffff
}

fn send_packet_no_acknowledge(address: DeviceAddress, request: &[u8]) {
    IRQ_STAT.write(!(1 << IRQ_SIO0));
    SIO_CTRL0.set_bits(SIO_CTRL_DTR | SIO_CTRL_ACKNOWLEDGE);
    delay_microseconds(DTR_DELAY);

    SIO_DATA0.write(address as u8);
    delay_microseconds(BYTE_DELAY);
    drain_rx_fifo();

    for &byte in request {
        exchange_byte(byte);
        delay_microseconds(BYTE_DELAY);
    }

    delay_microseconds(DTR_DELAY);
    SIO_CTRL0.clear_bits(SIO_CTRL_DTR);
}

/// Returns a bitmask of the ports with a memory card that supports game IDs.
pub fn check_mcp_present() -> u8 {
    let request = [CMD_GAME_ID_PING, 0, 0, 0, 0];
    let request_id = [CMD_CARD_IDENTIFY];
    let mut ports = 0;

    for port in 0..2 {
        let mut response = [0u8; 5];
        let mut response_id = [0u8; 9];

        select_port(port);
        exchange_packet(DeviceAddress::MemoryCard, &request_id, &mut response_id);

        let resp_length = exchange_packet(DeviceAddress::MemoryCard, &request, &mut response);

        if resp_length == 5 && response[3] == 0x27 && response[4] == 0xff {
            ports |= 1 << port;
        }
    }

    ports
}

/// Sends a NUL-terminated game ID to the memory cards on the ports in `card`.
pub fn send_game_id(game_id: &[u8], card: u8) {
    let mut request = [0u8; 3 + MAX_GAME_ID_LENGTH + 1];
    let length = game_id.len() + 1;

    request[0] = CMD_GAME_ID_SEND;
    request[1] = 0;
    request[2] = length as u8;
    for (byte, &id_byte) in request[3..].iter_mut().zip(game_id) {
        *byte = id_byte;
    }

    let request = &request[..(length + 3).min(request.len())];

    for port in 0..2 {
        if card & (1 << port) != 0 {
            select_port(port);
            send_packet_no_acknowledge(DeviceAddress::MemoryCard, request);
        }
    }
}
