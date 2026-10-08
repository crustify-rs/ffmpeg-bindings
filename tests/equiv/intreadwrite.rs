//! Equivalence of `libavutil/intreadwrite.h`'s `unaligned_N` wrappers with
//! C's `union unaligned_N`, on independently owned buffers.
//!
//! The unions have no C routine of their own: `AV_RN(N, p)` / `AV_WN(N, p, v)`
//! expand inline to a cast of a byte pointer to `union unaligned_N *` and a
//! read or store of `l`. The C references are those expressions over the raw
//! `ffi::unaligned_N` layouts, and `av_memcpy_backptr`, whose fills and short
//! copies go through the unions at whatever alignment `dst` has: `fill32`'s
//! 32-byte blocks through `AV_WN64`; `fill16` (`AV_RN16`, then `AV_WN32`),
//! `fill24` (`AV_WN32`), `fill32`'s tail (`AV_RN32`, `AV_WN32`) and the short
//! copy (`AV_COPY32U` / `AV_COPY16U`) through the 16- and 32-bit unions.
//!
//! The `av_alias*` wrappers are compared with the raw C unions: the same
//! write sequence applied to an independent raw union and to the safe value,
//! every view compared after each step.

use core::mem::{align_of, offset_of, size_of};

use ffibox::CZeroable;
use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::intreadwrite::{
    Alias16, Alias16Ref, Alias32, Alias32Ref, Unaligned16, Unaligned16Mut, Unaligned16Ref,
    Unaligned32, Unaligned32Mut, Unaligned32Ref, Unaligned64, Unaligned64Mut, Unaligned64Ref,
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

// `Unaligned16` and `Unaligned32`.

/// `AV_RN(16, p)`.
fn c_rn16(buf: &[u8], off: usize) -> u16 {
    assert!(off + 2 <= buf.len());
    // SAFETY: `off + 2 <= buf.len()`, so the 2-byte, align-1 union lies in
    // `buf`; reading a packed field by value copies it unaligned.
    unsafe { (*buf.as_ptr().add(off).cast::<ffi::unaligned_16>()).l }
}

/// `AV_WN(16, p, v)`.
fn c_wn16(buf: &mut [u8], off: usize, v: u16) {
    assert!(off + 2 <= buf.len());
    // SAFETY: as `c_rn16`, through the exclusive borrow; assigning a packed
    // field stores it unaligned.
    unsafe { (*buf.as_mut_ptr().add(off).cast::<ffi::unaligned_16>()).l = v }
}

/// `AV_RN(32, p)`.
fn c_rn32(buf: &[u8], off: usize) -> u32 {
    assert!(off + 4 <= buf.len());
    // SAFETY: as `c_rn16`, for the 4-byte union.
    unsafe { (*buf.as_ptr().add(off).cast::<ffi::unaligned_32>()).l }
}

/// `AV_WN(32, p, v)`.
fn c_wn32(buf: &mut [u8], off: usize, v: u32) {
    assert!(off + 4 <= buf.len());
    // SAFETY: as `c_wn16`, for the 4-byte union.
    unsafe { (*buf.as_mut_ptr().add(off).cast::<ffi::unaligned_32>()).l = v }
}

fn rs_rn16(buf: &[u8], off: usize) -> u16 {
    Unaligned16Ref::from_bytes(buf[off..].first_chunk().unwrap()).value()
}

fn rs_wn16(buf: &mut [u8], off: usize, v: u16) {
    Unaligned16Mut::from_bytes(buf[off..].first_chunk_mut().unwrap()).set_value(v)
}

fn rs_rn32(buf: &[u8], off: usize) -> u32 {
    Unaligned32Ref::from_bytes(buf[off..].first_chunk().unwrap()).value()
}

fn rs_wn32(buf: &mut [u8], off: usize, v: u32) {
    Unaligned32Mut::from_bytes(buf[off..].first_chunk_mut().unwrap()).set_value(v)
}

/// The bytes of a raw union value, as C lays them out.
fn c_bytes<T: Copy, const N: usize>(v: T) -> [u8; N] {
    assert_eq!(size_of::<T>(), N);
    // SAFETY: `T` is one of the packed unions, `N` bytes with every byte
    // initialised by its integer member.
    unsafe { core::mem::transmute_copy(&v) }
}

const VALUES_16: [u16; 6] = [0, 1, 0x00ff, 0xff00, 0x1234, u16::MAX];
const VALUES_32: [u32; 7] = [0, 1, 0xff, 0xff00_0000, 0x0102_0304, 0x8000_0001, u32::MAX];

#[test]
fn layout_matches_the_c_unions() {
    assert_eq!(size_of::<Unaligned16>(), size_of::<ffi::unaligned_16>());
    assert_eq!(align_of::<Unaligned16>(), align_of::<ffi::unaligned_16>());
    assert_eq!(size_of::<Unaligned32>(), size_of::<ffi::unaligned_32>());
    assert_eq!(align_of::<Unaligned32>(), align_of::<ffi::unaligned_32>());
    assert_eq!(
        (size_of::<Unaligned16>(), align_of::<Unaligned16>()),
        (2, 1)
    );
    assert_eq!(
        (size_of::<Unaligned32>(), align_of::<Unaligned32>()),
        (4, 1)
    );
}

#[test]
fn values_hold_the_c_bytes() {
    for v in VALUES_16 {
        let c: [u8; 2] = c_bytes(ffi::unaligned_16 { l: v });
        let r: [u8; 2] = c_bytes(Unaligned16::new(v));
        assert_eq!(r, c, "{v:#x}");
        assert_eq!(c_bytes::<_, 2>(Unaligned16::from(v)), c);
    }
    for v in VALUES_32 {
        let c: [u8; 4] = c_bytes(ffi::unaligned_32 { l: v });
        let r: [u8; 4] = c_bytes(Unaligned32::new(v));
        assert_eq!(r, c, "{v:#x}");
        assert_eq!(c_bytes::<_, 4>(Unaligned32::from(v)), c);
    }
    assert_eq!(
        c_bytes::<_, 2>(Unaligned16::default()),
        c_bytes::<_, 2>(ffi::unaligned_16::default())
    );
    assert_eq!(
        c_bytes::<_, 4>(Unaligned32::default()),
        c_bytes::<_, 4>(ffi::unaligned_32::default())
    );
}

#[test]
fn reads_match_at_every_offset_of_the_same_bytes() {
    let buf: Vec<u8> = (0..23u8).map(|i| i.wrapping_mul(37) ^ 0x5a).collect();
    for off in 0..=buf.len() - 2 {
        assert_eq!(rs_rn16(&buf, off), c_rn16(&buf, off), "offset {off}");
    }
    for off in 0..=buf.len() - 4 {
        assert_eq!(rs_rn32(&buf, off), c_rn32(&buf, off), "offset {off}");
    }
}

#[test]
fn write_sequences_leave_twin_buffers_identical_after_each_step() {
    let mut c = vec![0xa5u8; 19];
    let mut r = c.clone();
    let mut step = 0u32;
    for round in 0..3 {
        for off in (0..=c.len() - 4).rev() {
            let v = VALUES_32[(off + round) % VALUES_32.len()] ^ step;
            c_wn32(&mut c, off, v);
            rs_wn32(&mut r, off, v);
            assert_eq!(r, c, "wn32 step {step} offset {off}");
            assert_eq!(rs_rn32(&r, off), c_rn32(&c, off));
            step += 1;
        }
        for off in 0..=c.len() - 2 {
            let v = VALUES_16[(off + round) % VALUES_16.len()] ^ step as u16;
            c_wn16(&mut c, off, v);
            rs_wn16(&mut r, off, v);
            assert_eq!(r, c, "wn16 step {step} offset {off}");
            // Each side reads what the other's store left.
            assert_eq!(rs_rn16(&c, off), c_rn16(&r, off));
            assert_eq!(
                rs_rn32(&c, off.min(c.len() - 4)),
                c_rn32(&r, off.min(c.len() - 4))
            );
            step += 1;
        }
    }
}

#[test]
fn a_store_through_one_side_reads_back_through_the_other() {
    let mut buf = [0u8; 9];
    for (off, v) in VALUES_32
        .iter()
        .copied()
        .enumerate()
        .map(|(i, v)| (i % 6, v))
    {
        rs_wn32(&mut buf, off, v);
        assert_eq!(c_rn32(&buf, off), v);
        c_wn32(&mut buf, off, !v);
        assert_eq!(rs_rn32(&buf, off), !v);
    }
    for (off, v) in VALUES_16.iter().copied().enumerate() {
        rs_wn16(&mut buf, off, v);
        assert_eq!(c_rn16(&buf, off), v);
        c_wn16(&mut buf, off + 1, v.rotate_left(4));
        assert_eq!(rs_rn16(&buf, off + 1), v.rotate_left(4));
    }
}

#[test]
fn inline_values_match_the_c_union_field() {
    let mut r = Unaligned32::new(0);
    let mut c = ffi::unaligned_32 { l: 0 };
    for v in VALUES_32 {
        r.as_mut().set_value(v);
        c.l = v;
        // SAFETY: `l` is the union's only member, initialised just above.
        let cl = unsafe { c.l };
        assert_eq!(r.as_ref().value(), cl);
        assert_eq!(c_bytes::<_, 4>(r), c_bytes::<_, 4>(c));
    }
    let mut r = Unaligned16::new(0);
    let mut c = ffi::unaligned_16 { l: 0 };
    for v in VALUES_16 {
        r.as_mut().set_value(v);
        c.l = v;
        // SAFETY: `l` is the union's only member, initialised just above.
        let cl = unsafe { c.l };
        assert_eq!(u16::from(r), cl);
        assert_eq!(c_bytes::<_, 2>(r), c_bytes::<_, 2>(c));
    }
}

/// Rust: `av_memcpy_backptr` at the periods whose C goes through the 16- and
/// 32-bit unions, each read and store through `Unaligned16*` /
/// `Unaligned32*` where C uses the union: `fill16` (period 2), `fill24`
/// (period 3), `fill32` below its 64-bit blocks (period 4, under 32 bytes),
/// and the short copy (period 5 or more, under 16 bytes).
fn rust_backptr_narrow(buf: &mut [u8], dst: usize, back: usize, cnt: usize) {
    let (mut d, mut len) = (dst, cnt);
    match back {
        2 | 4 => {
            let v = if back == 2 {
                let v = u32::from(rs_rn16(buf, dst - 2));
                v | v << 16
            } else {
                assert!(cnt < 32, "fill32 stores 32-byte blocks through AV_WN64");
                rs_rn32(buf, dst - 4)
            };
            while len >= 4 {
                rs_wn32(buf, d, v);
                d += 4;
                len -= 4;
            }
            for i in d..d + len {
                buf[i] = buf[i - back];
            }
        }
        3 => {
            // `fill24`'s seed is `AV_RL24`, a byte-wise read; its stores go
            // through the union. The little-endian rotations, as on this
            // build (`HAVE_BIGENDIAN` 0).
            let v = u32::from_le_bytes([buf[dst - 3], buf[dst - 2], buf[dst - 1], 0]);
            let (a, b, c) = (v | v << 24, v >> 8 | v << 16, v >> 16 | v << 8);
            while len >= 12 {
                rs_wn32(buf, d, a);
                rs_wn32(buf, d + 4, b);
                rs_wn32(buf, d + 8, c);
                d += 12;
                len -= 12;
            }
            for word in [a, b] {
                if len >= 4 {
                    rs_wn32(buf, d, word);
                    d += 4;
                    len -= 4;
                }
            }
            for i in d..d + len {
                buf[i] = buf[i - 3];
            }
        }
        5.. => {
            assert!(cnt < 16, "longer copies are memcpy blocks");
            let mut s = dst - back;
            if len >= 8 {
                let v = rs_rn32(buf, s);
                rs_wn32(buf, d, v);
                let v = rs_rn32(buf, s + 4);
                rs_wn32(buf, d + 4, v);
                (s, d, len) = (s + 8, d + 8, len - 8);
            }
            if len >= 4 {
                let v = rs_rn32(buf, s);
                rs_wn32(buf, d, v);
                (s, d, len) = (s + 4, d + 4, len - 4);
            }
            if len >= 2 {
                let v = rs_rn16(buf, s);
                rs_wn16(buf, d, v);
                (s, d, len) = (s + 2, d + 2, len - 2);
            }
            if len > 0 {
                buf[d] = buf[s];
            }
        }
        _ => unreachable!("periods 0 and 1 never reach the unions"),
    }
}

#[test]
fn backptr_narrow_paths_match_rust_views_at_every_alignment() {
    for (back, counts) in [
        (2, 0..40),
        (3, 0..40),
        (4, 0..32),
        (5, 0..16),
        (6, 0..16),
        (7, 0..16),
        (8, 0..16),
        (11, 0..16),
        (15, 0..16),
    ] {
        for cnt in counts {
            for dst in back..back + 8 {
                let mut c = pattern(dst + cnt + 3);
                let mut rust = c.clone();
                c_backptr(&mut c, dst, back, cnt);
                rust_backptr_narrow(&mut rust, dst, back, cnt);
                assert_eq!(c, rust, "back {back} cnt {cnt} at {dst}");
            }
        }
    }
}

#[test]
fn c_fills_read_rust_seeds_and_rust_reads_c_fills() {
    let mut c = pattern(64);
    let mut rust = c.clone();

    // 1. Rust seeds a period-2 fill at an odd offset on both sides.
    for buf in [&mut c, &mut rust] {
        rs_wn16(buf, 3, 0xbeef);
    }
    assert_eq!(c, rust);

    // 2. C fills from the seed; Rust mirrors it.
    c_backptr(&mut c, 5, 2, 23);
    rust_backptr_narrow(&mut rust, 5, 2, 23);
    assert_eq!(c, rust);
    for off in 3..5 + 23 - 2 {
        assert_eq!(rs_rn16(&c, off), c_rn16(&rust, off), "offset {off}");
    }
    let [lo, hi] = 0xbeef_u16.to_ne_bytes();
    assert_eq!(rs_rn32(&c, 7), u32::from_ne_bytes([lo, hi, lo, hi]));

    // 3. Rust overwrites a word that seeds a period-4 fill, then C fills on.
    for buf in [&mut c, &mut rust] {
        rs_wn32(buf, 29, 0x0102_0304);
    }
    c_backptr(&mut c, 33, 4, 27);
    rust_backptr_narrow(&mut rust, 33, 4, 27);
    assert_eq!(c, rust);
    for off in (33..33 + 24).step_by(4) {
        assert_eq!(rs_rn32(&c, off), 0x0102_0304, "offset {off}");
    }

    // 4. A short copy reads back across what both fills wrote.
    c_backptr(&mut c, 60, 9, 4);
    rust_backptr_narrow(&mut rust, 60, 9, 4);
    assert_eq!(c, rust);
    assert_eq!(rs_rn32(&c, 60), c_rn32(&c, 51));
}
