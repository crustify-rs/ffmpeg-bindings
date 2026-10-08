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
