//! Equivalence of `libavutil/intreadwrite.h`'s `Unaligned64` with C's
//! `union unaligned_64`, on independently owned buffers.
//!
//! The C reference is `av_memcpy_backptr` with a 4-byte period, whose `fill32`
//! stores each 32-byte block as four `AV_WN64` writes through the union at
//! whatever alignment `dst` has, and the `AV_RN64` / `AV_WN64` expressions
//! themselves over the raw `ffi::unaligned_64` layout.
//!
//! The `av_alias*` wrappers are compared with the raw C unions: the same
//! write sequence applied to an independent raw union and to the safe value,
//! every view compared after each step.

use core::mem::{align_of, offset_of, size_of};

use ffibox::CZeroable;
use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::intreadwrite::{
    Alias16, Alias16Ref, Alias32, Alias32Ref, Unaligned64, Unaligned64Mut, Unaligned64Ref,
};
use ffmpeg::libavutil::mem::HeapBox;

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

/// Every view of a raw `av_alias16`.
fn raw16(a: &ffi::av_alias16) -> (u16, [u8; 2]) {
    // SAFETY: both fields cover all bytes of an initialised union, and any
    // bit pattern is valid for each.
    unsafe { (a.u16_, a.u8_) }
}

/// Every view of a raw `av_alias32`; the float as its bits, so NaNs compare.
fn raw32(a: &ffi::av_alias32) -> (u32, [u16; 2], [u8; 4], u32) {
    // SAFETY: every field covers all bytes of an initialised union, and any
    // bit pattern is valid for each.
    unsafe { (a.u32_, a.u16_, a.u8_, a.f32_.to_bits()) }
}

fn safe16(a: Alias16Ref<'_>) -> (u16, [u8; 2]) {
    (a.u16(), a.u8x2())
}

fn safe32(a: Alias32Ref<'_>) -> (u32, [u16; 2], [u8; 4], u32) {
    (a.u32(), a.u16x2(), a.u8x4(), a.f32().to_bits())
}

#[derive(Clone, Copy, Debug)]
enum Write32 {
    U32(u32),
    U16(usize, u16),
    U8(usize, u8),
    F32(f32),
}

/// A write sequence through every view, each lane and byte singly, and the
/// float's special values.
const STEPS32: &[Write32] = &[
    Write32::U32(0x0102_0304),
    Write32::U16(0, 0xaaaa),
    Write32::U8(3, 0x55),
    Write32::F32(-1.0),
    Write32::U8(0, 0x01),
    Write32::U16(1, 0x7fc0),
    Write32::F32(f32::INFINITY),
    Write32::U8(1, 0xff),
    Write32::U8(2, 0x00),
    Write32::F32(f32::MIN_POSITIVE),
    Write32::U32(0),
    Write32::F32(-0.0),
];

#[derive(Clone, Copy, Debug)]
enum Write16 {
    U16(u16),
    U8(usize, u8),
}

/// A write sequence through both views, each byte singly.
const STEPS16: &[Write16] = &[
    Write16::U16(0x1234),
    Write16::U8(0, 0xff),
    Write16::U8(1, 0x00),
    Write16::U16(u16::MAX),
    Write16::U8(1, 0x80),
    Write16::U16(0),
];

#[test]
fn layouts_match() {
    assert_eq!(size_of::<Alias16>(), size_of::<ffi::av_alias16>());
    assert_eq!(align_of::<Alias16>(), align_of::<ffi::av_alias16>());
    assert_eq!(size_of::<Alias32>(), size_of::<ffi::av_alias32>());
    assert_eq!(align_of::<Alias32>(), align_of::<ffi::av_alias32>());
}

#[test]
fn zeroed_matches_a_zeroed_union() {
    assert_eq!(
        safe16(Alias16::zeroed().as_ref()),
        raw16(&ffi::av_alias16::default())
    );
    assert_eq!(
        safe32(Alias32::default().as_ref()),
        raw32(&ffi::av_alias32::default())
    );
}

#[test]
fn alias32_write_sequence_matches_the_union() {
    let mut raw = ffi::av_alias32::default();
    let mut value = Alias32::zeroed();
    let mut boxed = HeapBox::<Alias32>::new_zeroed().unwrap();
    for (i, &step) in STEPS32.iter().enumerate() {
        // Read-modify-write the lanes and bytes as the C macros would, the
        // union field by field and the wrappers through their array views.
        match step {
            Write32::U32(v) => {
                raw.u32_ = v;
                value.as_mut().set_u32(v);
                boxed.as_mut().set_u32(v);
            }
            Write32::U16(lane, v) => {
                // SAFETY: the lane view covers all bytes of an initialised union.
                unsafe { raw.u16_[lane] = v };
                for mut m in [value.as_mut(), boxed.as_mut()] {
                    let mut lanes = m.as_ref().u16x2();
                    lanes[lane] = v;
                    m.set_u16x2(lanes);
                }
            }
            Write32::U8(byte, v) => {
                // SAFETY: the byte view covers all bytes of an initialised union.
                unsafe { raw.u8_[byte] = v };
                for mut m in [value.as_mut(), boxed.as_mut()] {
                    let mut bytes = m.as_ref().u8x4();
                    bytes[byte] = v;
                    m.set_u8x4(bytes);
                }
            }
            Write32::F32(v) => {
                raw.f32_ = v;
                value.as_mut().set_f32(v);
                boxed.as_mut().set_f32(v);
            }
        }
        let want = raw32(&raw);
        assert_eq!(
            safe32(value.as_ref()),
            want,
            "value after step {i}: {step:?}"
        );
        assert_eq!(safe32(boxed.as_ref()), want, "box after step {i}: {step:?}");
    }
}

#[test]
fn alias16_write_sequence_matches_the_union() {
    let mut raw = ffi::av_alias16::default();
    let mut value = Alias16::zeroed();
    let mut boxed = HeapBox::<Alias16>::new_zeroed().unwrap();
    for (i, &step) in STEPS16.iter().enumerate() {
        match step {
            Write16::U16(v) => {
                raw.u16_ = v;
                value.as_mut().set_u16(v);
                boxed.as_mut().set_u16(v);
            }
            Write16::U8(byte, v) => {
                // SAFETY: the byte view covers all bytes of an initialised union.
                unsafe { raw.u8_[byte] = v };
                for mut m in [value.as_mut(), boxed.as_mut()] {
                    let mut bytes = m.as_ref().u8x2();
                    bytes[byte] = v;
                    m.set_u8x2(bytes);
                }
            }
        }
        let want = raw16(&raw);
        assert_eq!(
            safe16(value.as_ref()),
            want,
            "value after step {i}: {step:?}"
        );
        assert_eq!(safe16(boxed.as_ref()), want, "box after step {i}: {step:?}");
    }
}

#[test]
fn constructors_match_a_union_written_through_the_same_field() {
    for v in [0, 1, 0x8000_0001, u32::MAX] {
        assert_eq!(
            safe32(Alias32::from_u32(v).as_ref()),
            raw32(&ffi::av_alias32 { u32_: v })
        );
    }
    for v in [[0, 0], [1, 0xffff], [0x1234, 0x5678]] {
        assert_eq!(
            safe32(Alias32::from_u16x2(v).as_ref()),
            raw32(&ffi::av_alias32 { u16_: v })
        );
    }
    for v in [[0; 4], [1, 2, 3, 4], [0xff, 0, 0x80, 0x7f]] {
        assert_eq!(
            safe32(Alias32::from_u8x4(v).as_ref()),
            raw32(&ffi::av_alias32 { u8_: v })
        );
    }
    for v in [
        0.0,
        -0.0,
        1.5,
        f32::NAN,
        f32::NEG_INFINITY,
        f32::from_bits(1),
    ] {
        assert_eq!(
            safe32(Alias32::from_f32(v).as_ref()),
            raw32(&ffi::av_alias32 { f32_: v })
        );
    }
    for v in [0, 0x00ff, u16::MAX] {
        assert_eq!(
            safe16(Alias16::from_u16(v).as_ref()),
            raw16(&ffi::av_alias16 { u16_: v })
        );
    }
    for v in [[0, 0], [0xff, 0x01]] {
        assert_eq!(
            safe16(Alias16::from_u8x2(v).as_ref()),
            raw16(&ffi::av_alias16 { u8_: v })
        );
    }
}

#[test]
fn heap_box_clone_matches_av_memdup() {
    let mut boxed = HeapBox::<Alias32>::new_zeroed().unwrap();
    boxed.as_mut().set_u32(0xcafe_f00d);
    let raw = ffi::av_alias32 { u32_: 0xcafe_f00d };
    // SAFETY: `raw` is readable for its size; `av_memdup` only reads it and
    // returns a fresh block or NULL.
    let dup = unsafe { ffi::av_memdup((&raw const raw).cast(), size_of::<ffi::av_alias32>()) };
    assert!(!dup.is_null());
    // SAFETY: a non-NULL `dup` holds a byte copy of an initialised union,
    // aligned by `av_malloc`.
    let c_copy = raw32(&unsafe { dup.cast::<ffi::av_alias32>().read() });
    // SAFETY: `dup` is a live `av_malloc`-family block we own.
    unsafe { ffi::av_free(dup) };
    assert_eq!(safe32(boxed.clone().as_ref()), c_copy);
}
