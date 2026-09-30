/*
 * Lays out a PS-EXE: a 2 KB header followed by the program, loaded at
 * 0x80010000 (the first 64 KB of RAM belong to the kernel). The header fields
 * are computed here, so the linker's raw binary output boots as is.
 */

ENTRY(_start)

MEMORY {
	HEADER  (r)   : ORIGIN = 0x8000f800, LENGTH = 0x800
	APP_RAM (rwx) : ORIGIN = 0x80010000, LENGTH = 0x1f0000
}

SECTIONS {
	.psexe_header : {
		BYTE(0x50) BYTE(0x53) BYTE(0x2d) BYTE(0x58) /* "PS-X EXE" */
		BYTE(0x20) BYTE(0x45) BYTE(0x58) BYTE(0x45)
		QUAD(0)
		LONG(_start)              /* Entry point */
		LONG(0)                   /* Initial $gp */
		LONG(_textStart)          /* Load address */
		LONG(_exeEnd - _textStart) /* Load size, a multiple of 2 KB */
		QUAD(0)                   /* Data section (unused) */
		QUAD(0)                   /* BSS section (cleared by _start) */
		LONG(0)                   /* Stack, taken from SYSTEM.CNF */
		LONG(0)
		. = ALIGN(0x800);
	} > HEADER

	.text : {
		_textStart = .;

		*(.text .text.*)
	} > APP_RAM

	.rodata : {
		*(.rodata .rodata.*)
	} > APP_RAM

	.data : {
		*(.data .data.* .sdata .sdata.*)
		*(.got)

		. = ALIGN(0x800);
		_exeEnd = .;
	} > APP_RAM

	.bss (NOLOAD) : {
		_bssStart = .;

		*(.sbss .sbss.* .bss .bss.*)
		*(COMMON)

		. = ALIGN(4);
		_bssEnd = .;
	} > APP_RAM

	/DISCARD/ : {
		*(.MIPS.abiflags .reginfo .pdr .mdebug.* .eh_frame .eh_frame_hdr .comment .note.*)
	}
}
