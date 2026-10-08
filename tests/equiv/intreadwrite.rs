//! Equivalence of `libavutil/intreadwrite.h`'s `Unaligned64` with C's
//! `union unaligned_64`, on independently owned buffers.
//!
//! The C reference is `av_memcpy_backptr` with a 4-byte period, whose `fill32`
//! stores each 32-byte block as four `AV_WN64` writes through the union at
//! whatever alignment `dst` has, and the `AV_RN64` / `AV_WN64` expressions
//! themselves over the raw `ffi::unaligned_64` layout.

use core::mem::{align_of, offset_of, size_of};

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::intreadwrite::{Unaligned64, Unaligned64Mut, Unaligned64Ref};

/// The 8-byte window of `buf` at `off`.
fn window(buf: &[u8], off: usize) -> Unaligned64Ref<'_> {
    Unaligned64Ref::from_bytes(buf[off..off + 8].try_into().unwrap())
}

/// The exclusive 8-byte window of `buf` at `off`.
fn window_mut(buf: &mut [u8], off: usize) -> Unaligned64Mut<'_> {
    Unaligned64Mut::from_bytes((&mut buf[off..off + 8]).try_into().unwrap())
}

/// `AV_RN64(p)`: `((const union unaligned_64 *)(p))->l`.
fn c_rn64(buf: &[u8], off: usize) -> u64 {
    assert!(off + 8 <= buf.len());
    let p = buf[off..].as_ptr().cast::<ffi::unaligned_64>();
    // SAFETY: `p` addresses 8 initialised bytes of `buf`; the union has
    // alignment 1, and reading a packed field by value is an unaligned load,
    // as C's is.
    unsafe { (*p).l }
}

/// `AV_WN64(p, v)`: `((union unaligned_64 *)(p))->l = v`.
fn c_wn64(buf: &mut [u8], off: usize, v: u64) {
    assert!(off + 8 <= buf.len());
    let p = buf[off..].as_mut_ptr().cast::<ffi::unaligned_64>();
    // SAFETY: `p` addresses 8 writable bytes of `buf`, borrowed exclusively;
    // assigning a packed field is an unaligned store, as C's is.
    unsafe { (*p).l = v }
}

/// C: `av_memcpy_backptr(buf + dst, back, cnt)`.
fn c_backptr(buf: &mut [u8], dst: usize, back: usize, cnt: usize) {
    assert!(back <= dst && dst + cnt <= buf.len());
    let p = buf[dst..].as_mut_ptr();
    // SAFETY: `p` and the `back` bytes before it lie in `buf`, and so do the
    // `cnt` bytes it writes; `buf` is borrowed exclusively for the call.
    unsafe { ffi::av_memcpy_backptr(p, back as i32, cnt as i32) }
}

/// Rust: the same fill at a period of 4, the 32-byte blocks stored through
/// `Unaligned64Mut` as `fill32` stores them through the union, the tail
/// copied byte by byte.
fn rust_fill32(buf: &mut [u8], dst: usize, cnt: usize) {
    let seed: [u8; 4] = buf[dst - 4..dst].try_into().unwrap();
    let v = u64::from(u32::from_ne_bytes(seed));
    let v2 = v + (v << 32);
    let blocks = cnt / 32 * 32;
    for off in (dst..dst + blocks).step_by(8) {
        window_mut(buf, off).set_value(v2);
    }
    for i in dst + blocks..dst + cnt {
        buf[i] = buf[i - 4];
    }
}

/// A buffer of `len` distinct bytes.
fn pattern(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 37 + 11) as u8).collect()
}

#[test]
fn layout_matches_c() {
    assert_eq!(size_of::<Unaligned64>(), size_of::<ffi::unaligned_64>());
    assert_eq!(align_of::<Unaligned64>(), align_of::<ffi::unaligned_64>());
    assert_eq!(
        (size_of::<Unaligned64>(), align_of::<Unaligned64>()),
        (8, 1)
    );
    assert_eq!(offset_of!(ffi::unaligned_64, l), 0);
}

#[test]
fn reads_and_writes_match_the_c_macros_at_every_offset() {
    let mut c = pattern(40);
    let mut rust = c.clone();
    for off in 0..=32 {
        assert_eq!(window(&rust, off).value(), c_rn64(&c, off), "read at {off}");
        let v = c_rn64(&c, off).rotate_left(off as u32 + 1) ^ 0xa5a5_5a5a_0ff0_f00f;
        c_wn64(&mut c, off, v);
        window_mut(&mut rust, off).set_value(v);
        assert_eq!(c, rust, "write at {off}");
        assert_eq!(window(&rust, off).value(), c_rn64(&c, off));
    }
}

#[test]
fn values_match_the_c_union_by_value() {
    for v in [0, 1, u64::MAX, 0x0123_4567_89ab_cdef, 1 << 63] {
        let mut ours = Unaligned64::new(v);
        let mut theirs = ffi::unaligned_64 { l: v };
        // SAFETY: `theirs` was built through its only field.
        let l = unsafe { theirs.l };
        assert_eq!(ours.as_ref().value(), l);
        ours.as_mut().set_value(!v);
        theirs.l = !v;
        // SAFETY: as above, after a write of the same field.
        let l = unsafe { theirs.l };
        assert_eq!(ours.as_ref().value(), l);
    }
}

#[test]
fn backptr_fill_matches_rust_writes_at_every_alignment() {
    // 32, 64 and 96 run `fill32`'s `AV_WN64` blocks only; 37 and 75 add the
    // 32-bit and byte tails; 31 and 3 never reach `AV_WN64` (baseline).
    for cnt in [3, 31, 32, 37, 64, 75, 96] {
        for dst in 4..12 {
            let mut c = pattern(dst + cnt + 5);
            let mut rust = c.clone();
            c_backptr(&mut c, dst, 4, cnt);
            rust_fill32(&mut rust, dst, cnt);
            assert_eq!(c, rust, "cnt {cnt} at {dst}");
        }
    }
}

#[test]
fn c_fills_and_rust_writes_interleave() {
    let len = 160;
    let mut c = pattern(len);
    let mut rust = c.clone();

    // 1. C fills from a 4-byte seed at an odd offset.
    c_backptr(&mut c, 5, 4, 64);
    rust_fill32(&mut rust, 5, 64);
    assert_eq!(c, rust);

    // 2. Every window C wrote reads the same through the safe view, and the
    // period-4 fill makes windows 4 bytes apart equal.
    for off in 1..=5 + 64 - 8 {
        assert_eq!(window(&c, off).value(), window(&rust, off).value());
        assert_eq!(window(&c, off).value(), c_rn64(&c, off));
    }
    assert_eq!(window(&c, 9).value(), window(&c, 13).value());

    // 3. Rust overwrites an unaligned window on both sides, which seeds the
    // next fill: its last 4 bytes precede `dst`.
    for buf in [&mut c, &mut rust] {
        window_mut(buf, 67).set_value(0x1122_3344_5566_7788);
    }
    assert_eq!(c, rust);

    // 4. C fills on from there, reading the seed Rust wrote.
    c_backptr(&mut c, 75, 4, 75);
    rust_fill32(&mut rust, 75, 75);
    assert_eq!(c, rust);
    let seed = u32::from_ne_bytes(c[71..75].try_into().unwrap());
    let v2 = u64::from(seed) * 0x1_0000_0001;
    assert_eq!(window(&c, 75).value(), v2);
    assert_eq!(window(&c, 75 + 64).value(), v2);
}
