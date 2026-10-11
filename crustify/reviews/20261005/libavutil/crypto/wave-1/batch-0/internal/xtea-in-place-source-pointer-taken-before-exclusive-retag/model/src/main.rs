//! The old `xtea.rs::process` pointer derivation for an in-place call, with
//! C's per-block "read src, then write dst" replaced by Rust raw accesses.
fn c_like_crypt(dst: *mut u8, src: *const u8, len: usize) {
    for i in 0..len {
        // SAFETY: the model's caller passes `len` bytes behind both pointers.
        unsafe { *dst.add(i) = (*src.add(i)).wrapping_add(1) };
    }
}

fn process(dst: &mut [u8], src: Option<&[u8]>) {
    let src_ptr = src.map_or_else(|| dst.as_ptr(), <[u8]>::as_ptr);
    c_like_crypt(dst.as_mut_ptr(), src_ptr, dst.len());
}

fn main() {
    let mut data = [1u8; 16];
    process(&mut data, None);
    println!("{data:?}");
}
