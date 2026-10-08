//! Equivalence of `libavutil/intfloat.h`'s `IntFloat64` with C's
//! `union av_intfloat64`, whose reinterpretation `av_int2double` and
//! `av_double2int` perform in C, on independently owned values.

use core::mem::{align_of, offset_of, size_of};

use ffibox::CZeroable;
use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::intfloat::{IntFloat64, IntFloat64Mut, IntFloat64Ref};
use ffmpeg::libavutil::mem::{HeapBox, alloc_cell_array_zeroed};

/// Bit patterns at every class edge of a `double`, NaN payloads included, and
/// a few ordinary values.
const BITS: [u64; 16] = [
    0,
    1 << 63,
    1,
    0x000f_ffff_ffff_ffff,
    0x0010_0000_0000_0000,
    0x3ff0_0000_0000_0000,
    0xc004_0000_0000_0000,
    0x4009_21fb_5444_2d18,
    0x7fef_ffff_ffff_ffff,
    0x7ff0_0000_0000_0000,
    0xfff0_0000_0000_0000,
    0x7ff0_0000_0000_0001,
    0x7ff4_0000_0000_0000,
    0x7ff8_0000_0000_0000,
    0xfff8_dead_beef_0001,
    u64::MAX,
];

/// C's `{ .i = bits }.f`.
fn c_int2double(bits: u64) -> f64 {
    // SAFETY: `av_int2double` takes its argument by value and has no
    // preconditions.
    unsafe { ffi::av_int2double(bits) }
}

/// C's `{ .f = value }.i`.
fn c_double2int(value: f64) -> u64 {
    // SAFETY: `av_double2int` takes its argument by value and has no
    // preconditions.
    unsafe { ffi::av_double2int(value) }
}

/// The C view of a union Rust holds, from a byte copy: the reference reads
/// its members as C does, `i` directly and `f` through `av_int2double`.
fn c_views(r: IntFloat64Ref<'_>) -> (u64, u64) {
    let i = r.bits();
    (i, c_int2double(i).to_bits())
}

/// Both Rust views of `r`, the float one as its bit pattern.
fn rust_views(r: IntFloat64Ref<'_>) -> (u64, u64) {
    (r.bits(), r.float().to_bits())
}

#[test]
fn layout_matches_the_c_union() {
    assert_eq!(size_of::<IntFloat64>(), size_of::<ffi::av_intfloat64>());
    assert_eq!(align_of::<IntFloat64>(), align_of::<ffi::av_intfloat64>());
    assert_eq!(offset_of!(ffi::av_intfloat64, i), 0);
    assert_eq!(offset_of!(ffi::av_intfloat64, f), 0);
}

#[test]
fn integer_member_reads_as_c_int2double() {
    for bits in BITS {
        let v = IntFloat64::from_bits(bits);
        assert_eq!(
            v.as_ref().float().to_bits(),
            c_int2double(bits).to_bits(),
            "{bits:#x}"
        );
        assert_eq!(v.as_ref().bits(), bits);
    }
}

#[test]
fn double_member_reads_as_c_double2int() {
    for bits in BITS {
        let value = c_int2double(bits);
        let v = IntFloat64::from_float(value);
        assert_eq!(v.as_ref().bits(), c_double2int(value), "{bits:#x}");
        assert_eq!(v.as_ref().float().to_bits(), value.to_bits());
    }
}

#[test]
fn zeroed_union_matches_c_zero_initialiser() {
    let c = ffi::av_intfloat64::default();
    // SAFETY: `default` zero-fills the union; `i` covers all of it.
    let c_bits = unsafe { c.i };
    for v in [IntFloat64::zeroed(), IntFloat64::default()] {
        assert_eq!(
            rust_views(v.as_ref()),
            (c_bits, c_int2double(c_bits).to_bits())
        );
    }
}

/// One step of a write sequence, applied to the Rust handle and to a C
/// union, then compared member by member.
#[derive(Clone, Copy, Debug)]
enum Step {
    Bits(u64),
    Float(u64),
}

fn apply(m: &mut IntFloat64Mut<'_>, c: &mut ffi::av_intfloat64, step: Step) {
    match step {
        Step::Bits(bits) => {
            m.set_bits(bits);
            c.i = bits;
        }
        Step::Float(bits) => {
            let value = c_int2double(bits);
            m.set_float(value);
            c.f = value;
        }
    }
}

fn steps() -> impl Iterator<Item = Step> {
    BITS.into_iter()
        .zip(BITS.into_iter().rev())
        .flat_map(|(a, b)| [Step::Bits(a), Step::Float(b), Step::Float(a), Step::Bits(b)])
}

fn compare(r: IntFloat64Ref<'_>, c: &ffi::av_intfloat64, step: Step) {
    // SAFETY: both members of a C union span its 8 initialised bytes and
    // admit every bit pattern, whichever was written last.
    let (ci, cf) = unsafe { (c.i, c.f.to_bits()) };
    assert_eq!(rust_views(r), (ci, cf), "after {step:?}");
    assert_eq!(c_views(r), (ci, cf), "after {step:?}");
    assert_eq!(c_double2int(r.float()), ci, "after {step:?}");
}

#[test]
fn write_sequences_on_an_inline_value_track_a_c_union() {
    let mut v = IntFloat64::zeroed();
    let mut c = ffi::av_intfloat64::default();
    for step in steps() {
        let mut m = v.as_mut();
        apply(&mut m, &mut c, step);
        compare(m.as_ref(), &c, step);
        compare(v.as_ref(), &c, step);
    }
}

#[test]
fn write_sequences_on_a_heap_box_track_a_c_union() {
    let mut b = HeapBox::<IntFloat64>::new_zeroed().unwrap();
    let mut c = ffi::av_intfloat64::default();
    for step in steps() {
        apply(&mut b.as_mut(), &mut c, step);
        compare(b.as_ref(), &c, step);
        // A clone (`av_memdup`) carries the same word, as a C copy does.
        let copy = b.clone();
        let c_copy = c;
        compare(copy.as_ref(), &c_copy, step);
    }
}

#[test]
fn heap_array_elements_track_c_unions() {
    let mut a = alloc_cell_array_zeroed::<IntFloat64>(BITS.len()).unwrap();
    let mut c = [ffi::av_intfloat64::default(); BITS.len()];
    for (i, step) in steps().enumerate() {
        let slot = i % BITS.len();
        apply(
            &mut a.as_handles_mut().get_mut(slot).unwrap(),
            &mut c[slot],
            step,
        );
        for (r, c) in a.as_handles().iter().zip(&c) {
            compare(r, c, step);
        }
    }
}

#[test]
fn copies_diverge_as_c_copies_do() {
    let mut a = IntFloat64::from_bits(BITS[7]);
    let mut ca = ffi::av_intfloat64 { i: BITS[7] };
    let b = a;
    let cb = ca;
    apply(&mut a.as_mut(), &mut ca, Step::Float(BITS[5]));
    compare(a.as_ref(), &ca, Step::Float(BITS[5]));
    compare(b.as_ref(), &cb, Step::Bits(BITS[7]));
}
