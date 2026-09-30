/// Aligns its contents for DMA, which transfers whole 32-bit words.
#[repr(C, align(8))]
pub struct Aligned<T: ?Sized>(pub T);
