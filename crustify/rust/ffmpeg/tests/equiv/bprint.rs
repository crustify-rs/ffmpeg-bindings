//! Equivalence of `FF_PAD_STRUCTURE`'s [`PaddedStruct`] description of
//! `AVBPrint` with the structure size `bprint.c` observes and writes.

use core::ffi::c_char;
use core::mem::{MaybeUninit, offset_of};
use core::ptr::{addr_of, addr_of_mut};

use ffmpeg::libavutil::bprint::PaddedStruct;
use ffmpeg::libavutil::ffi;

const SIZE: usize = <ffi::AVBPrint as PaddedStruct>::SIZE;

/// The C-side state of an `AVBPrint`: `(str, len, size, size_max)`.
fn state(buf: *const ffi::AVBPrint) -> (*mut c_char, u32, u32, u32) {
    // SAFETY: per each caller, `buf` addresses an `AVBPrint` that
    // `av_bprint_init` initialised; the fields are read by value.
    unsafe {
        (
            addr_of!((*buf).str_).read(),
            addr_of!((*buf).len).read(),
            addr_of!((*buf).size).read(),
            addr_of!((*buf).size_max).read(),
        )
    }
}

/// The `size` bytes of the buffer at `str`, which the caller keeps alive.
fn contents<'a>(str: *const c_char, size: u32) -> &'a [u8] {
    // SAFETY: per each caller, `str` is the internal buffer of a live
    // `AVBPrint`, `size` bytes that C has written up to its NUL and that
    // nothing writes while the view lives.
    unsafe { core::slice::from_raw_parts(str.cast(), size as usize) }
}

/// `av_bprint_init(buf, 0, AV_BPRINT_SIZE_AUTOMATIC)` on Rust storage.
fn init_automatic(storage: &mut MaybeUninit<ffi::AVBPrint>) -> *mut ffi::AVBPrint {
    let buf = storage.as_mut_ptr();
    // SAFETY: `buf` is `size_of::<AVBPrint>()` writable bytes, which
    // `av_bprint_init` initialises in place without reading them first.
    unsafe { ffi::av_bprint_init(buf, 0, ffi::AV_BPRINT_SIZE_AUTOMATIC) };
    buf
}

#[test]
fn automatic_storage_is_the_padded_size() {
    let mut storage = MaybeUninit::uninit();
    let buf = init_automatic(&mut storage);
    let (str, len, size, size_max) = state(buf);

    // C sizes the internal buffer as `sizeof(AVBPrint)` past its start.
    let internal = offset_of!(ffi::AVBPrint, reserved_internal_buffer);
    assert_eq!(size as usize, SIZE - internal);
    assert_eq!((len, size_max), (0, size));
    // SAFETY: only the field's address is taken, through the raw pointer.
    let internal_buffer = unsafe { addr_of_mut!((*buf).reserved_internal_buffer) };
    assert_eq!(str, internal_buffer.cast());
    assert_eq!(contents(str, 1), b"\0");
}

#[test]
fn c_fills_the_storage_to_its_last_byte_and_no_further() {
    let mut storage = MaybeUninit::uninit();
    let buf = init_automatic(&mut storage);
    let (str, _, size, _) = state(buf);
    let room = size - 1;

    // Step by step to the brim: everything stays in the structure.
    let mut expect = Vec::new();
    for n in [100, room - 100 - 1, 1] {
        // SAFETY: `buf` is an initialised automatic `AVBPrint`, which
        // `av_bprint_chars` appends to within its `size`.
        unsafe { ffi::av_bprint_chars(buf, b'x' as c_char, n) };
        expect.resize(expect.len() + n as usize, b'x');
        let (s, len, sz, _) = state(buf);
        assert_eq!((s, len, sz), (str, expect.len() as u32, size));
        assert_eq!(
            &contents(str, size)[..=expect.len()],
            [&expect[..], b"\0"].concat()
        );
    }
    // The NUL lands on the structure's last byte, `SIZE - 1`.
    assert_eq!(expect.len(), room as usize);
    let last = str.wrapping_add(room as usize) as usize - buf as usize;
    assert_eq!(last, SIZE - 1);

    // One more is counted but truncated: nothing is written past `SIZE`.
    // SAFETY: as above; a full automatic buffer cannot grow, so C truncates.
    unsafe { ffi::av_bprint_chars(buf, b'y' as c_char, 1) };
    let (s, len, sz, _) = state(buf);
    assert_eq!((s, len, sz), (str, size, size));
    assert_eq!(contents(str, size), [&expect[..], b"\0"].concat());
}

/// Equivalence of [`BPrintFields`], `struct ff_pad_helper_AVBPrint`, with the
/// leading fields of the `AVBPrint` that `bprint.c` initialises and fills: C
/// lays both out alike (the padding only follows the fields), so the header
/// C writes, read as a helper, is what a `BPrintFields` holding C's values
/// reports and stores.
mod fields {
    use core::ffi::{CStr, c_char};
    use core::mem::{MaybeUninit, size_of};

    use ffmpeg::libavutil::bprint::{BPrintFields, PaddedStruct};
    use ffmpeg::libavutil::ffi;

    use super::state;

    /// The fields C wrote at the start of `buf`, read through the helper's
    /// layout.
    fn c_header(buf: *const ffi::AVBPrint) -> ffi::ff_pad_helper_AVBPrint {
        // SAFETY: per each caller, `buf` addresses an `AVBPrint` that
        // `av_bprint_init` initialised. Its first `FIELDS_SIZE` bytes hold
        // the helper's fields at the helper's offsets; the helper's trailing
        // alignment, the only bytes C may have left unwritten, is padding.
        unsafe { buf.cast::<ffi::ff_pad_helper_AVBPrint>().read() }
    }

    /// The raw fields a `BPrintFields` stores.
    fn rust_raw(f: &BPrintFields) -> ffi::ff_pad_helper_AVBPrint {
        // SAFETY: `BPrintFields` is `#[repr(transparent)]` over
        // `ff_pad_helper_AVBPrint`, and `f` is a live, initialised value.
        unsafe { core::ptr::from_ref(f).cast::<ffi::ff_pad_helper_AVBPrint>().read() }
    }

    /// The `BPrintFields` holding the state C keeps in `buf`.
    ///
    /// # Safety
    ///
    /// `buf` is an initialised `AVBPrint` whose string stays unwritten and
    /// alive while the result or a copy reads it.
    unsafe fn mirror(buf: *const ffi::AVBPrint) -> BPrintFields {
        let (str, len, size, size_max) = state(buf);
        let h = c_header(buf);
        let mut f = BPrintFields::new();
        let mut m = f.as_mut();
        // SAFETY: C keeps `str` NUL-terminated within its storage, the
        // internal buffer of `buf`, which the caller keeps alive and
        // unwritten while `f` can read it.
        unsafe { m.set_str_borrowed(Some(CStr::from_ptr(str))) };
        m.set_len(len);
        m.set_size(size);
        m.set_size_max(size_max);
        m.set_internal_buffer([h.reserved_internal_buffer[0] as u8]);
        f
    }

    /// Compares C's header with the helper read of it and with its mirror.
    fn compare(buf: *const ffi::AVBPrint) {
        let (str, len, size, size_max) = state(buf);
        let h = c_header(buf);
        assert_eq!((h.str_, h.len, h.size, h.size_max), (str, len, size, size_max));

        // SAFETY: `buf` is live and initialised, and nothing appends to it
        // while `f` lives.
        let f = unsafe { mirror(buf) };
        let r = f.as_ref();
        assert_eq!((r.len(), r.size(), r.size_max()), (len, size, size_max));
        assert_eq!(r.internal_buffer(), [h.reserved_internal_buffer[0] as u8]);
        // SAFETY: as for `mirror`: C's string is NUL-terminated and alive.
        let c_str = unsafe { CStr::from_ptr(str) };
        assert_eq!(r.str(), Some(c_str));

        let raw = rust_raw(&f);
        assert_eq!(raw.str_, h.str_);
        assert_eq!((raw.len, raw.size, raw.size_max), (h.len, h.size, h.size_max));
        assert_eq!(raw.reserved_internal_buffer, h.reserved_internal_buffer);
    }

    /// `av_bprint_init(buf, 0, size_max)` on Rust storage.
    fn init(storage: &mut MaybeUninit<ffi::AVBPrint>, size_max: u32) -> *mut ffi::AVBPrint {
        let buf = storage.as_mut_ptr();
        // SAFETY: `buf` is `size_of::<AVBPrint>()` writable bytes, which
        // `av_bprint_init` initialises in place; `size_init` 0 never
        // allocates.
        unsafe { ffi::av_bprint_init(buf, 0, size_max) };
        buf
    }

    #[test]
    fn the_helper_is_the_header_size_c_pads() {
        assert_eq!(
            size_of::<BPrintFields>(),
            <ffi::AVBPrint as PaddedStruct>::FIELDS_SIZE
        );
        assert_eq!(
            size_of::<BPrintFields>() + size_of::<[c_char; 1000]>(),
            size_of::<ffi::AVBPrint>()
        );
    }

    #[test]
    fn automatic_buffers_match_after_every_append() {
        let mut storage = MaybeUninit::uninit();
        let buf = init(&mut storage, ffi::AV_BPRINT_SIZE_AUTOMATIC);
        compare(buf);
        // The first byte of the internal buffer moves from NUL to text; the
        // last appends truncate.
        for (c, n) in [(b'a', 1), (b'b', 20), (b'c', 980), (b'd', 2), (b'e', 1000)] {
            // SAFETY: `buf` is an initialised automatic `AVBPrint`, which
            // `av_bprint_chars` appends to within its `size`.
            unsafe { ffi::av_bprint_chars(buf, c as c_char, n) };
            compare(buf);
        }
        let (_, len, size, _) = state(buf);
        assert_eq!((len, size), (2003, 1004));
    }

    #[test]
    fn count_only_buffers_match_after_every_append() {
        let mut storage = MaybeUninit::uninit();
        // `AV_BPRINT_SIZE_COUNT_ONLY`: C counts, but writes only the NUL.
        let buf = init(&mut storage, 0);
        compare(buf);
        for n in [1, 4096, u32::MAX] {
            // SAFETY: `buf` is an initialised count-only `AVBPrint`, which
            // `av_bprint_chars` only counts into.
            unsafe { ffi::av_bprint_chars(buf, b'z' as c_char, n) };
            compare(buf);
        }
        // C stops 5 short of `UINT_MAX`.
        let (_, len, size, size_max) = state(buf);
        assert_eq!((len, size, size_max), (u32::MAX - 5, 0, 0));
    }
}
