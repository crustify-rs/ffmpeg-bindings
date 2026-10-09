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

/// Equivalence of the print buffer owners with `av_bprint_init`,
/// `av_bprint_init_for_buffer` and `av_bprint_finalize` on independently
/// owned C buffers: the state each getter reads after every step of a
/// sequence of C appends, clears and truncations, the string each finalizer
/// hands out, and what a caller buffer holds afterwards.
mod owners {
    use core::ffi::{CStr, c_char, c_uint};
    use core::mem::MaybeUninit;
    use core::ptr::{self, addr_of, addr_of_mut};

    use ffmpeg::libavutil::bprint::{BPrint, BPrintMut, BPrintRef};
    use ffmpeg::libavutil::ffi;

    /// A C-side buffer on its own heap storage, which C initialises in place
    /// and which never moves.
    struct CBuf {
        buf: *mut ffi::AVBPrint,
        /// Whether its string may be finalized (not a caller buffer).
        owned: bool,
    }

    impl CBuf {
        fn storage() -> *mut ffi::AVBPrint {
            Box::into_raw(Box::new(MaybeUninit::<ffi::AVBPrint>::uninit())).cast()
        }

        fn init(size_init: c_uint, size_max: c_uint) -> Self {
            let buf = Self::storage();
            // SAFETY: `buf` is writable storage for one `AVBPrint`.
            unsafe { ffi::av_bprint_init(buf, size_init, size_max) };
            Self { buf, owned: true }
        }

        fn for_buffer(data: &mut [u8]) -> Self {
            let buf = Self::storage();
            // SAFETY: `buf` is writable storage, `data` writable for its
            // length, which outlives every use of `buf` in these tests.
            unsafe {
                ffi::av_bprint_init_for_buffer(buf, data.as_mut_ptr().cast(), data.len() as c_uint)
            };
            Self { buf, owned: false }
        }

        /// `(len, size, size_max, inline, stored bytes, string up to NUL)`.
        fn state(&self) -> State {
            let b = self.buf;
            // SAFETY: `b` is an initialised buffer; fields are read by value,
            // and its string is valid for `size` bytes, written up to its NUL
            // at `min(len, size - 1)`.
            unsafe {
                let str = addr_of!((*b).str_).read();
                let len = addr_of!((*b).len).read();
                let size = addr_of!((*b).size).read();
                let stored = len.min(size.saturating_sub(1)) as usize;
                let bytes = if stored == 0 {
                    Vec::new()
                } else {
                    core::slice::from_raw_parts(str.cast::<u8>(), stored).to_vec()
                };
                let c_str = if size == 0 {
                    Vec::new()
                } else {
                    CStr::from_ptr(str).to_bytes().to_vec()
                };
                State {
                    len,
                    size,
                    size_max: addr_of!((*b).size_max).read(),
                    inline: ptr::eq(str, addr_of_mut!((*b).reserved_internal_buffer).cast()),
                    bytes,
                    c_str,
                }
            }
        }

        /// `av_bprint_finalize(buf, &str)`: its return and the string's
        /// stored bytes, without the NUL (`None` for a count-only buffer,
        /// whose block holds no string).
        fn finalize(mut self) -> (i32, Option<Vec<u8>>) {
            let mut str = ptr::null_mut();
            // SAFETY: an owned, initialised buffer; `str` a writable slot.
            let ret = unsafe { ffi::av_bprint_finalize(self.buf, &mut str) };
            // SAFETY: `size` is the string's length, NUL included.
            let real = unsafe { addr_of!((*self.buf).size).read() } as usize;
            let bytes = (ret == 0 && real > 0).then(|| {
                // SAFETY: C's fresh block holds `real` bytes, the last a NUL.
                unsafe { core::slice::from_raw_parts(str.cast::<u8>(), real - 1) }.to_vec()
            });
            // SAFETY: C's block, ours to free (or NULL).
            unsafe { ffi::av_free(str.cast()) };
            self.owned = false;
            (ret, bytes)
        }
    }

    impl Drop for CBuf {
        fn drop(&mut self) {
            if self.owned {
                // SAFETY: an owned buffer, whose string C frees.
                unsafe { ffi::av_bprint_finalize(self.buf, ptr::null_mut()) };
            }
            // SAFETY: the storage `Box` that `storage` leaked.
            drop(unsafe { Box::from_raw(self.buf.cast::<MaybeUninit<ffi::AVBPrint>>()) });
        }
    }

    #[derive(Debug, PartialEq)]
    struct State {
        len: u32,
        size: u32,
        size_max: u32,
        inline: bool,
        bytes: Vec<u8>,
        c_str: Vec<u8>,
    }

    fn rust_state(r: BPrintRef<'_>) -> State {
        State {
            len: r.len(),
            size: r.size(),
            size_max: r.size_max(),
            inline: r.is_inline(),
            bytes: r.as_bytes().to_vec(),
            c_str: r.as_c_str().to_bytes().to_vec(),
        }
    }

    /// One step, applied alike to both buffers.
    #[derive(Clone, Copy, Debug)]
    enum Op {
        Append(&'static [u8]),
        Chars(u8, u32),
        Clear,
        Truncate(u32),
    }

    /// Appends binary data with NULs, grows past the inline storage,
    /// truncates within and past the stored part, clears, and appends again.
    const OPS: &[Op] = &[
        Op::Append(b"head\0bin\x01"),
        Op::Chars(b'x', 3),
        Op::Truncate(20),
        Op::Truncate(6),
        Op::Chars(b'y', 1500),
        Op::Append(b"tail"),
        Op::Truncate(1003),
        Op::Truncate(1499),
        Op::Clear,
        Op::Clear,
        Op::Append(b"\0lead-nul"),
        Op::Chars(b'z', 0),
        Op::Truncate(0),
        Op::Chars(b'w', 2),
    ];

    /// `op` on a C buffer, truncation as C callers write it (avcodec.c).
    fn c_op(b: *mut ffi::AVBPrint, op: Op) {
        // SAFETY: `b` is an initialised buffer, which these routines append
        // to within its `size`; the truncation writes within `size`.
        unsafe {
            match op {
                Op::Append(d) => {
                    ffi::av_bprint_append_data(b, d.as_ptr().cast(), d.len() as c_uint)
                }
                Op::Chars(c, n) => ffi::av_bprint_chars(b, c as c_char, n),
                Op::Clear => ffi::av_bprint_clear(b),
                Op::Truncate(n) => {
                    if n < (*b).len {
                        (*b).len = n;
                        if (*b).size > 0 {
                            *(*b).str_.add(n.min((*b).size - 1) as usize) = 0;
                        }
                    }
                }
            }
        }
    }

    /// `op` on a Rust buffer: C's appends through the owner's pointer, the
    /// truncation through the safe setter.
    fn rust_op(b: *mut ffi::AVBPrint, m: &mut BPrintMut<'_>, op: Op) {
        match op {
            Op::Truncate(n) => m.truncate(n),
            _ => c_op(b, op),
        }
    }

    const INITS: [(u32, u32); 9] = [
        (0, BPrint::SIZE_UNLIMITED),
        (0, BPrint::SIZE_AUTOMATIC),
        (0, BPrint::SIZE_COUNT_ONLY),
        (0, 10),
        (0, 1500),
        (2000, BPrint::SIZE_UNLIMITED),
        (5000, 3000),
        (1, 2),
        (u32::MAX, BPrint::SIZE_UNLIMITED),
    ];

    #[test]
    fn new_and_its_finalizers_match_av_bprint_init_and_finalize() {
        for (size_init, size_max) in INITS {
            let c = CBuf::init(size_init, size_max);
            let mut r = BPrint::new(size_init, size_max).unwrap();
            let ctx = format!("init({size_init}, {size_max})");
            assert_eq!(rust_state(r.as_ref()), c.state(), "{ctx}");
            for &op in OPS {
                let p = r.as_c_ptr();
                c_op(c.buf, op);
                rust_op(p, &mut r.as_mut(), op);
                assert_eq!(rust_state(r.as_ref()), c.state(), "{ctx} {op:?}");
            }
            let expect = c.state();

            // `into_bytes` against C's finalized string.
            let (ret, c_bytes) = c.finalize();
            assert_eq!(ret, 0);
            match BPrint::into_bytes(r) {
                Ok(bytes) => {
                    assert_eq!(
                        bytes.as_slice(),
                        c_bytes.as_deref().unwrap_or_default(),
                        "{ctx}"
                    );
                    assert_eq!(bytes.as_slice(), expect.bytes);
                }
                Err(e) => panic!("{ctx}: {e}"),
            }

            // `into_string` likewise, after the same sequence.
            let c = CBuf::init(size_init, size_max);
            let mut r = BPrint::new(size_init, size_max).unwrap();
            for &op in OPS {
                let p = r.as_c_ptr();
                c_op(c.buf, op);
                rust_op(p, &mut r.as_mut(), op);
            }
            let (ret, c_bytes) = c.finalize();
            let s = BPrint::into_string(r).unwrap();
            assert_eq!(ret, 0);
            match c_bytes {
                // C's string, read up to its NUL.
                Some(b) => assert_eq!(
                    s.as_bytes(),
                    CStr::from_bytes_until_nul(&[&b[..], b"\0"].concat())
                        .unwrap()
                        .to_bytes()
                ),
                // A count-only buffer: C's block holds no string, the
                // documented divergence; Rust hands back "".
                None => {
                    assert_eq!(expect.size, 0, "{ctx}");
                    assert_eq!(s.as_bytes(), b"");
                }
            }
        }
    }

    #[test]
    fn for_buffer_matches_av_bprint_init_for_buffer() {
        for len in [0, 1, 2, 5, 64, 2048] {
            let mut c_data = vec![0xa5_u8; len];
            let mut r_data = vec![0xa5_u8; len];
            let c = CBuf::for_buffer(&mut c_data);
            let mut r = BPrint::for_buffer(&mut r_data).unwrap();
            assert_eq!(rust_state(r.as_ref()), c.state(), "for_buffer({len})");
            for &op in OPS {
                let p = r.as_c_ptr();
                c_op(c.buf, op);
                rust_op(p, &mut r.as_mut(), op);
                assert_eq!(
                    rust_state(r.as_ref()),
                    c.state(),
                    "for_buffer({len}) {op:?}"
                );
            }
            drop(r);
            drop(c);
            assert_eq!(r_data, c_data, "for_buffer({len})");
        }
    }
    /// Whether this is the isolated child of test `name`; in the parent, runs
    /// the child, asserts it passed, and returns `false`.
    fn isolated(name: &str) -> bool {
        const ISOLATED: &str = "CRUSTIFY_EQUIV_ISOLATED";
        if std::env::var_os(ISOLATED).is_some() {
            return true;
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", name, "--test-threads=1", "--nocapture"])
            .env(ISOLATED, "1")
            .status()
            .unwrap();
        assert!(status.success(), "isolated run failed: {status}");
        false
    }

    /// Fills both buffers with `n` bytes of `c`.
    fn fill(c: &CBuf, r: &mut ffmpeg::libavutil::bprint::BPrintBox, n: u32) {
        c_op(c.buf, Op::Chars(b'f', n));
        c_op(r.as_c_ptr(), Op::Chars(b'f', n));
    }

    /// The finalizers' allocation failures, forced by the `av_max_alloc`
    /// limit in an isolated child: an inline string C cannot copy out fails
    /// alike with `AVERROR(ENOMEM)`, and a grown string C cannot shrink is
    /// handed out unshrunk, alike.
    #[test]
    fn finalizing_at_the_av_max_alloc_limit_matches_c() {
        if !isolated("bprint::owners::finalizing_at_the_av_max_alloc_limit_matches_c") {
            return;
        }
        let enomem = -(ffi::ENOMEM as i32);
        // A C and a Rust buffer, both unlimited, starting inline.
        let pair = || {
            (
                CBuf::init(0, BPrint::SIZE_UNLIMITED),
                BPrint::new(0, BPrint::SIZE_UNLIMITED).unwrap(),
            )
        };

        let (c, mut r) = pair();
        fill(&c, &mut r, 100);
        ffmpeg::libavutil::mem::set_max_alloc(50);
        assert_eq!(c.finalize(), (enomem, None));
        assert_eq!(BPrint::into_string(r).map(drop), Err(enomem));
        ffmpeg::libavutil::mem::set_max_alloc(i32::MAX as usize);

        let (c, mut r) = pair();
        fill(&c, &mut r, 100);
        ffmpeg::libavutil::mem::set_max_alloc(50);
        assert_eq!(c.finalize(), (enomem, None));
        assert_eq!(BPrint::into_bytes(r).map(drop), Err(enomem));
        ffmpeg::libavutil::mem::set_max_alloc(i32::MAX as usize);

        for into_string in [false, true] {
            let (c, mut r) = pair();
            fill(&c, &mut r, 3000);
            assert!(!r.as_ref().is_inline());
            ffmpeg::libavutil::mem::set_max_alloc(100);
            let (ret, c_bytes) = c.finalize();
            assert_eq!(ret, 0);
            let c_bytes = c_bytes.unwrap();
            assert_eq!(c_bytes.len(), 3000);
            if into_string {
                assert_eq!(BPrint::into_string(r).unwrap().as_bytes(), c_bytes);
            } else {
                assert_eq!(BPrint::into_bytes(r).unwrap().as_slice(), c_bytes);
            }
            ffmpeg::libavutil::mem::set_max_alloc(i32::MAX as usize);
        }

        // Rust's count-only string, "" where C has no string, needs one
        // byte; at a zero limit it fails as C's own copies do.
        let r = BPrint::new(0, BPrint::SIZE_COUNT_ONLY).unwrap();
        ffmpeg::libavutil::mem::set_max_alloc(0);
        assert_eq!(BPrint::into_string(r).map(drop), Err(enomem));
        ffmpeg::libavutil::mem::set_max_alloc(i32::MAX as usize);
    }

    /// After a failed growth, later appends remain truncated even when the
    /// allocation limit is restored. Truncating below capacity permits a
    /// fresh growth; compare all fields and stored bytes after each step.
    #[test]
    fn failed_growth_stays_truncated_until_explicitly_shortened() {
        if !isolated("bprint::owners::failed_growth_stays_truncated_until_explicitly_shortened") {
            return;
        }
        let c = CBuf::init(0, BPrint::SIZE_UNLIMITED);
        let mut r = BPrint::new(0, BPrint::SIZE_UNLIMITED).unwrap();
        ffmpeg::libavutil::mem::set_max_alloc(1024);
        for n in [2000, 16] {
            fill(&c, &mut r, n);
            assert_eq!(rust_state(r.as_ref()), c.state());
            assert!(r.as_ref().len() >= r.as_ref().size());
        }
        ffmpeg::libavutil::mem::set_max_alloc(i32::MAX as usize);
        fill(&c, &mut r, 32);
        assert_eq!(rust_state(r.as_ref()), c.state());
        assert!(r.as_ref().is_inline());
        c_op(c.buf, Op::Truncate(3));
        r.as_mut().truncate(3);
        assert_eq!(rust_state(r.as_ref()), c.state());
        // First move inline bytes to the heap, then grow that allocation
        // twice: both the memcpy and realloc branches preserve the prefix.
        for n in [2000, 5000, 20_000] {
            fill(&c, &mut r, n);
            assert_eq!(rust_state(r.as_ref()), c.state());
            assert!(!r.as_ref().is_inline());
        }
        let (ret, bytes) = c.finalize();
        assert_eq!(ret, 0);
        assert_eq!(BPrint::into_bytes(r).unwrap().as_slice(), bytes.unwrap());
    }
}
