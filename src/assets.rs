//! Data embedded into the executable. The textures are 4bpp images and their
//! palettes, as produced by ps1-bare-metal's convertImage.py.

use crate::aligned::Aligned;

pub static FONT_TEXTURE: &Aligned<[u8]> =
    &Aligned(*include_bytes!("../assets/textures/fontTexture.dat"));
pub static FONT_PALETTE: &Aligned<[u8]> =
    &Aligned(*include_bytes!("../assets/textures/fontPalette.dat"));
pub static LOGO_TEXTURE: &Aligned<[u8]> =
    &Aligned(*include_bytes!("../assets/textures/logoTexture.dat"));
pub static LOGO_PALETTE: &Aligned<[u8]> =
    &Aligned(*include_bytes!("../assets/textures/logoPalette.dat"));

pub static CLICK_SFX: &Aligned<[u8]> = &Aligned(*include_bytes!("../assets/click.vag"));
pub static SLIDE_SFX: &Aligned<[u8]> = &Aligned(*include_bytes!("../assets/slide.vag"));
