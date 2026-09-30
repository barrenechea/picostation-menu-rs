use std::env;

fn main() {
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();

    println!("cargo:rerun-if-changed=psexe.ld");
    println!("cargo:rustc-link-arg-bins=-T{manifest_dir}/psexe.ld");
    // The linker script lays out the PS-EXE header, so the raw binary output is
    // the executable the BIOS loads.
    println!("cargo:rustc-link-arg-bins=--oformat=binary");
}
