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

//! Program entry point and panic handler.

use core::panic::PanicInfo;
use core::ptr;

unsafe extern "C" {
    #[link_name = "_bssStart"]
    static mut BSS_START: u8;
    #[link_name = "_bssEnd"]
    static mut BSS_END: u8;
}

#[unsafe(no_mangle)]
unsafe extern "C" fn _start() -> ! {
    unsafe {
        let start = &raw mut BSS_START;
        let end = &raw mut BSS_END;

        ptr::write_bytes(start, 0, end.addr() - start.addr());
    }

    crate::main()
}

#[panic_handler]
fn panic(_info: &PanicInfo<'_>) -> ! {
    loop {
        core::hint::spin_loop();
    }
}
