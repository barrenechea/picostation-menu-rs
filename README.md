# picostation-menu-rs

The menu disc for the [PicoStation](https://github.com/barrenechea/picostation) optical drive emulator, written in Rust. It's what the PlayStation boots into: it browses the folders and disc images on the PicoStation's SD card and mounts the one you pick.

This is a port of the C [picostation-menu](https://github.com/Team-Resurgent/picostation-menu) and behaves the same way. It runs bare metal on the console's R3000A: no SDK, no C toolchain and no dependencies beyond Rust's `core`.

## Controls

| Button | Action |
| --- | --- |
| Up / Down | Move through the list (hold to scroll) |
| Left / L1, Right / R1 | Previous / next page |
| X | Open the folder, or boot the game, skipping the BIOS intro |
| Start | Boot the game through the BIOS intro |
| Square | Go up to the parent folder |
| Triangle | Restart the PicoStation into its USB bootloader, for flashing firmware |
| Select | Show the credits |

Audio CDs always go through the BIOS, since only its CD player can play them. If a memory card that supports game IDs (such as a MemCard PRO) is connected, the menu sends it the game's ID before booting, so it can switch to that game's card.

## Building

You need:
- [rustup](https://rustup.rs). The nightly pinned in `rust-toolchain.toml` and its `rust-src` component are installed on the first build.
- CMake 3.25 or newer.
- [mkpsxiso](https://github.com/Lameguy64/mkpsxiso/releases) 2.30 or newer, to build the disc image.

```sh
cmake -S . -B build
cmake --build build
```

This produces `build/picostation-menu.bin` and `.cue`, the disc image the PicoStation firmware embeds. `cargo build --release` on its own builds just the PS-EXE, at `target/mipsel-sony-psx/release/picostation-menu.exe`.

### Toolchain

The PlayStation's target, `mipsel-sony-psx`, is a tier 3 Rust target, so it needs nightly and `-Zbuild-std` (both set up in `rust-toolchain.toml` and `.cargo/config.toml`). The pinned nightly must ship LLVM 22.1.5 or newer: older versions can move a load into a branch delay slot where its result gets used one instruction too early, as the R3000A doesn't stall for loads ([rust-lang/rust#150676](https://github.com/rust-lang/rust/issues/150676)). When bumping it, check the new build for load-delay hazards before using it.

## Credits

- [raijin](https://github.com/raijin), for [picostation-loader](https://github.com/raijin/picostation-loader), the original menu.
- EqUiNoX and [megavolt85](https://github.com/megavolt85), for [Team-Resurgent's picostation-menu](https://github.com/Team-Resurgent/picostation-menu), the C menu this is ported from.
- [spicyjpeg](https://github.com/spicyjpeg), for [ps1-bare-metal](https://github.com/spicyjpeg/ps1-bare-metal), which the hardware definitions and much of the code come from, and for the SPU driver from [573in1](https://github.com/spicyjpeg/573in1).
- [Rhys-Baker](https://github.com/Rhys-Baker), for [PSXProject](https://github.com/Rhys-Baker/PSXProject), the base of the CD-ROM, interrupt, filesystem and SPU code.
- [Lameguy64](https://github.com/Lameguy64), for mkpsxiso.
- Everyone the C menu thanks: Rama, Skitchin, Danhans42, NicholasNoble and the [PSX.Dev](https://www.psx.dev) community.
