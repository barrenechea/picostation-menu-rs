//! Globals, including those shared between the main loop and the interrupt
//! handler.

use core::arch::asm;
use core::cell::UnsafeCell;

/// A value that is always read from and written to memory, the equivalent of a
/// C `volatile` global. There is a single CPU core, and the only concurrency is
/// the interrupt handler preempting the main loop.
#[repr(transparent)]
pub struct Volatile<T>(UnsafeCell<T>);

unsafe impl<T> Sync for Volatile<T> {}

impl<T: Copy> Volatile<T> {
    pub const fn new(value: T) -> Self {
        Self(UnsafeCell::new(value))
    }

    #[inline(always)]
    pub fn get(&self) -> T {
        unsafe { self.0.get().read_volatile() }
    }

    #[inline(always)]
    pub fn set(&self, value: T) {
        unsafe { self.0.get().write_volatile(value) }
    }
}

/// A global accessed through raw pointers, for data too large for the stack
/// or filled in by DMA.
#[repr(transparent)]
pub struct StaticCell<T>(UnsafeCell<T>);

unsafe impl<T> Sync for StaticCell<T> {}

impl<T> StaticCell<T> {
    pub const fn new(value: T) -> Self {
        Self(UnsafeCell::new(value))
    }

    pub const fn get(&self) -> *mut T {
        self.0.get()
    }
}

/// Prevents the compiler from moving memory accesses across this point, so
/// that buffers are fully written before DMA reads them and are read only after
/// DMA or an interrupt handler has filled them.
#[inline(always)]
pub fn compiler_barrier() {
    unsafe { asm!("", options(nostack, preserves_flags)) }
}
