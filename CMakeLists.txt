cmake_minimum_required(VERSION 3.25)

# Builds picostation-menu.bin/.cue, the same disc image as menu/, from the Rust
# port. The executable itself is built by Cargo, so no C toolchain is needed.
project(
    picostation-menu
    LANGUAGES   NONE
    VERSION     1.0.0
    DESCRIPTION "PicoStation menu disc, Rust port"
)

find_program(
    CARGO_EXECUTABLE cargo
    HINTS "$ENV{HOME}/.cargo/bin"
    DOC   "Cargo, installed through rustup so that rust-toolchain.toml is honored"
    REQUIRED
)
find_program(
    MKPSXISO_EXECUTABLE mkpsxiso
    DOC "mkpsxiso executable (https://github.com/Lameguy64/mkpsxiso)"
    REQUIRED
)

set(CARGO_TARGET_DIR "${PROJECT_BINARY_DIR}/target")
set(PSEXE_PATH "${CARGO_TARGET_DIR}/mipsel-sony-psx/release/${PROJECT_NAME}.exe")

# Cargo tracks its own dependencies, so it runs on every build. RUSTFLAGS would
# replace the flags in .cargo/config.toml rather than add to them.
add_custom_target(
    psexe ALL
    COMMAND
        "${CMAKE_COMMAND}" -E env --unset=RUSTFLAGS --unset=CARGO_ENCODED_RUSTFLAGS
        "${CARGO_EXECUTABLE}" build --release --target-dir "${CARGO_TARGET_DIR}"
    WORKING_DIRECTORY "${PROJECT_SOURCE_DIR}"
    BYPRODUCTS "${PSEXE_PATH}"
    USES_TERMINAL
    VERBATIM
)

set(XISO_DIR "${PROJECT_SOURCE_DIR}/assets/xiso")
configure_file(
    "${XISO_DIR}/isoconfig.xml.in"
    "${PROJECT_BINARY_DIR}/isoconfig.xml"
    @ONLY
)

add_custom_command(
    OUTPUT
        ${PROJECT_NAME}.bin
        ${PROJECT_NAME}.cue
    COMMAND
        "${MKPSXISO_EXECUTABLE}" -y -q
        -o ${PROJECT_NAME}.bin
        -c ${PROJECT_NAME}.cue
        "${PROJECT_BINARY_DIR}/isoconfig.xml"
    DEPENDS
        psexe
        "${PSEXE_PATH}"
        "${PROJECT_BINARY_DIR}/isoconfig.xml"
        "${XISO_DIR}/LICENSEJ.DAT"
        "${XISO_DIR}/system.cnf"
        "${XISO_DIR}/PICO.DAT"
    VERBATIM
)
add_custom_target(image ALL DEPENDS ${PROJECT_NAME}.bin)
