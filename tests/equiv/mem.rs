//! Equivalence of `libavutil/mem.h`'s `av_malloc`-family owners with the raw
//! C allocator, on independently owned blocks; of `DynArray`,
//! `FastBuffer` and `ZeroedFastBuffer` with C `(tab, nb)` and `(ptr, size)`
//! pairs grown by the `av_dynarray` and `av_fast` families; and of the
//! allocation limit and back-reference copy with the C functions they wrap.
#![expect(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "test inputs are chosen within range, and the casts mirror the C conversions under comparison"
)]
#![expect(
    clippy::many_single_char_names,
    reason = "names follow the C fields, formulas and fixtures they mirror"
)]
#![expect(
    clippy::items_after_statements,
    clippy::needless_for_each,
    reason = "each test is one scenario, its helpers and steps kept where they are used"
)]

use core::ffi::{CStr, c_char, c_int, c_uint, c_void};
use core::marker::PhantomData;
use core::mem::{MaybeUninit, size_of};

use ffibox::{CMutSeam, CZeroable, define_ctype};
use ffmpeg_bindings::libavutil::libavutil::ffi;
use ffmpeg_bindings::libavutil::libavutil::mem::{
    BackReferenceError, DynArray, FastBuffer, HeapBox, HeapString, HeapVec, ZeroedFastBuffer,
    alloc_array, alloc_array_from_fn, alloc_array_zeroed, alloc_cell_array_zeroed,
    copy_back_reference, set_max_alloc, string_from_bytes_until_nul, string_from_c_str,
};

/// The alignment `av_malloc` guarantees in every configuration.
const ALIGN: usize = 16;

// `AVRational` owns nothing, so its bytes copy as a value (`HeapBox::clone`).
define_ctype!(
    #[derive(Clone, Copy)]
    Rational,
    RationalRef,
    RationalMut,
    ffi::AVRational
);
// SAFETY: 0/0 is a valid `AVRational` for these tests, which only compare its
// bytes.
unsafe impl CZeroable for Rational {}

/// A C block, freed with `av_free` on drop.
struct CBlock(*mut c_void);

impl CBlock {
    fn bytes(&self, len: usize) -> &[u8] {
        // SAFETY: the C allocation behind `self.0` holds at least `len`
        // initialised bytes, per each caller.
        unsafe { core::slice::from_raw_parts(self.0.cast(), len) }
    }
}

impl Drop for CBlock {
    fn drop(&mut self) {
        // SAFETY: `self.0` is NULL or a live `av_malloc`-family block we own.
        unsafe { ffi::av_free(self.0) }
    }
}

/// The bytes of `len` initialised `T` at `ptr`, which `owner` keeps alive.
fn rust_bytes<T, O>(owner: &O, ptr: *const T, len: usize) -> &[u8] {
    let _ = owner;
    // SAFETY: per each caller, `ptr` addresses `len * size_of::<T>()`
    // initialised bytes owned by `owner`, which the borrow keeps alive and
    // unwritten.
    unsafe { core::slice::from_raw_parts(ptr.cast(), len * size_of::<T>()) }
}

/// Element counts around every failure edge: the `av_max_alloc` limit
/// (`INT_MAX` bytes by default) and `size_t` overflow. Successes stay at or
/// under 4 KiB: an exact 2 GiB success would commit 2 GiB per side, and `BSan`'s
/// runtime grows without bound freeing a C block after a byte view of 32 KiB.
const COUNTS: [usize; 8] = [
    0,
    1,
    3,
    512,
    i32::MAX as usize + 1,
    usize::MAX / 2,
    usize::MAX / 2 + 1,
    usize::MAX,
];

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn alloc_array_fails_exactly_when_av_malloc_array_does() {
    fn check<T>() {
        for n in COUNTS {
            // SAFETY: `av_malloc_array` has no preconditions.
            let c = CBlock(unsafe { ffi::av_malloc_array(n, size_of::<T>()) });
            let r = alloc_array::<T>(n);
            assert_eq!(
                r.is_some(),
                !c.0.is_null(),
                "n = {n}, size = {}",
                size_of::<T>()
            );
            if let Some(mut r) = r {
                assert_eq!(r.len(), n);
                assert_eq!(r.as_ptr() as usize % ALIGN, 0);
                assert_eq!(c.0 as usize % ALIGN, 0);
                // Every slot is writable (a zero-sized array has none to write).
                if size_of::<T>() != 0 {
                    r.as_mut_slice().fill_with(MaybeUninit::zeroed);
                }
            }
        }
    }
    check::<u8>();
    check::<u32>();
    check::<[u8; 24]>();
    check::<()>();
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn alloc_array_zeroed_matches_av_calloc() {
    fn check<T: ffibox::CPlainElem>() {
        for n in COUNTS {
            // SAFETY: `av_calloc` has no preconditions.
            let c = CBlock(unsafe { ffi::av_calloc(n, size_of::<T>()) });
            let r = alloc_array_zeroed::<T>(n);
            assert_eq!(
                r.is_some(),
                !c.0.is_null(),
                "n = {n}, size = {}",
                size_of::<T>()
            );
            if let Some(r) = r {
                let len = n * size_of::<T>();
                assert_eq!(rust_bytes(&r, r.as_ptr(), n), c.bytes(len));
                assert!(c.bytes(len).iter().all(|&b| b == 0));
            }
        }
    }
    check::<u8>();
    check::<i64>();
    check::<[f32; 3]>();
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn cell_array_zeroed_matches_av_calloc() {
    for n in [0, 1, 7, usize::MAX / 8 + 1] {
        // SAFETY: `av_calloc` has no preconditions.
        let c = CBlock(unsafe { ffi::av_calloc(n, size_of::<ffi::AVRational>()) });
        let r = alloc_cell_array_zeroed::<Rational>(n);
        assert_eq!(r.is_some(), !c.0.is_null(), "n = {n}");
        if let Some(r) = r {
            let len = n * size_of::<ffi::AVRational>();
            assert_eq!(rust_bytes(&r, r.as_ptr(), n), c.bytes(len));
            assert_eq!(r.as_handles().len(), n);
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn alloc_array_from_fn_matches_a_filled_c_array() {
    let n = 100;
    // SAFETY: `av_malloc_array` has no preconditions.
    let c = CBlock(unsafe { ffi::av_malloc_array(n, size_of::<u32>()) });
    assert!(!c.0.is_null());
    for i in 0..n {
        // SAFETY: as the comment below states, for this part of the operation.
        let v_3 = unsafe { c.0.cast::<u32>().add(i) };
        // SAFETY: `c.0` holds `n` aligned `u32` slots.
        unsafe { v_3.write(i as u32 ^ 0x5A5A) };
    }
    let r = alloc_array_from_fn(n, |i| i as u32 ^ 0x5A5A).unwrap();
    assert_eq!(rust_bytes(&r, r.as_ptr(), n), c.bytes(n * 4));
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn from_slice_and_clone_match_av_memdup_step_by_step() {
    let mut src: Vec<u16> = (0..513).collect();
    let len = src.len() * 2;

    // Step 1: one copy each.
    // SAFETY: `src` is `len` readable bytes.
    let c1 = CBlock(unsafe { ffi::av_memdup(src.as_ptr().cast(), len) });
    let r1 = HeapVec::<u16>::from_slice(&src).unwrap();
    assert_eq!(rust_bytes(&r1, r1.as_ptr(), src.len()), c1.bytes(len));
    assert_eq!(
        (r1.as_ptr() as usize % ALIGN, c1.0 as usize % ALIGN),
        (0, 0)
    );

    // Step 2: the source changes; the copies do not.
    src.iter_mut().for_each(|x| *x = !*x);
    assert_eq!(rust_bytes(&r1, r1.as_ptr(), src.len()), c1.bytes(len));

    // Step 3: copy the copies.
    // SAFETY: `c1.0` holds `len` initialised bytes.
    let c2 = CBlock(unsafe { ffi::av_memdup(c1.0, len) });
    let mut r2 = r1.clone();
    assert_eq!(rust_bytes(&r2, r2.as_ptr(), src.len()), c2.bytes(len));
    assert_ne!(r2.as_ptr(), r1.as_ptr());

    // Step 4: write one copy; its source keeps its bytes.
    r2.as_mut_slice()[512] = 7;
    // SAFETY: as the comment below states, for this part of the operation.
    let v_2 = unsafe { c2.0.cast::<u16>().add(512) };
    // SAFETY: `c2.0` holds `len / 2` aligned `u16`s.
    unsafe { v_2.write(7) };
    assert_eq!(rust_bytes(&r2, r2.as_ptr(), src.len()), c2.bytes(len));
    assert_eq!(rust_bytes(&r1, r1.as_ptr(), src.len()), c1.bytes(len));
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn empty_copies_match_av_memdup_null_and_zero_length() {
    // SAFETY: NULL is accepted and yields NULL.
    assert!(unsafe { ffi::av_memdup(core::ptr::null(), 4) }.is_null());
    // A zero-length copy owns no C block on the Rust side.
    let r = HeapVec::<u8>::from_slice(&[]).unwrap();
    assert!(r.as_ptr().is_null() && r.clone().as_ptr().is_null());
    // C still allocates (`av_malloc(0)` is one byte); both are valid, empty.
    // SAFETY: zero bytes are read from a valid pointer.
    let c = CBlock(unsafe { ffi::av_memdup([0u8].as_ptr().cast(), 0) });
    assert!(!c.0.is_null());
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn new_zeroed_box_matches_av_mallocz() {
    // SAFETY: `av_mallocz` has no preconditions.
    let c = CBlock(unsafe { ffi::av_mallocz(size_of::<ffi::AVRational>()) });
    let r = HeapBox::<Rational>::new_zeroed().unwrap();
    assert_eq!(
        rust_bytes(&r, r.as_c_ptr(), 1),
        c.bytes(size_of::<ffi::AVRational>())
    );
    assert_eq!((r.as_ptr() as usize % ALIGN, c.0 as usize % ALIGN), (0, 0));
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn new_uninit_box_matches_av_malloc_and_fills_in_place() {
    // SAFETY: `av_malloc` has no preconditions.
    let c = CBlock(unsafe { ffi::av_malloc(size_of::<ffi::AVRational>()) });
    let mut slot = HeapBox::<Rational>::new_uninit().unwrap();
    let value = ffi::AVRational { num: -3, den: 8 };
    // For the unsafe operations below: both blocks are aligned and `AVRational`-sized, owned here.
    {
        // SAFETY: as the comment above this block states.
        unsafe { c.0.cast::<ffi::AVRational>().write(value) };
        // SAFETY: as the comment above this block states.
        unsafe { slot.as_c_ptr().write(value) };
    }
    // SAFETY: the write above initialised the whole `AVRational`.
    let r = unsafe { slot.assume_init() };
    assert_eq!(
        rust_bytes(&r, r.as_c_ptr(), 1),
        c.bytes(size_of::<ffi::AVRational>())
    );
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn box_clone_matches_av_memdup_step_by_step() {
    let size = size_of::<ffi::AVRational>();
    let value = ffi::AVRational { num: 7, den: -9 };

    // Step 1: one source each, holding the same value.
    // SAFETY: `av_malloc` has no preconditions.
    let c0 = CBlock(unsafe { ffi::av_malloc(size) });
    assert!(!c0.0.is_null());
    let mut r0 = HeapBox::<Rational>::new_zeroed().unwrap();
    // For the unsafe operations below: both blocks are aligned and `AVRational`-sized, owned here.
    {
        // SAFETY: as the comment above this block states.
        unsafe { c0.0.cast::<ffi::AVRational>().write(value) };
        // SAFETY: as the comment above this block states.
        unsafe { r0.as_mut().as_mut_ptr().write(value) };
    }
    assert_eq!(rust_bytes(&r0, r0.as_c_ptr(), 1), c0.bytes(size));

    // Step 2: one copy each.
    // SAFETY: `c0.0` holds `size` initialised bytes.
    let c1 = CBlock(unsafe { ffi::av_memdup(c0.0, size) });
    let r1 = r0.clone();
    assert_eq!(rust_bytes(&r1, r1.as_c_ptr(), 1), c1.bytes(size));
    assert_eq!(
        (r1.as_ptr() as usize % ALIGN, c1.0 as usize % ALIGN),
        (0, 0)
    );
    assert_ne!(r1.as_ptr(), r0.as_ptr());

    // Step 3: the sources change; the copies do not.
    let other = ffi::AVRational { num: 1, den: 1 };
    // For the unsafe operations below: as in step 1.
    {
        // SAFETY: as the comment above this block states.
        unsafe { c0.0.cast::<ffi::AVRational>().write(other) };
        // SAFETY: as the comment above this block states.
        unsafe { r0.as_mut().as_mut_ptr().write(other) };
    }
    assert_eq!(rust_bytes(&r0, r0.as_c_ptr(), 1), c0.bytes(size));
    assert_eq!(rust_bytes(&r1, r1.as_c_ptr(), 1), c1.bytes(size));

    // Step 4: copy the copies after the sources are gone.
    drop((c0, r0));
    // SAFETY: `c1.0` holds `size` initialised bytes.
    let c2 = CBlock(unsafe { ffi::av_memdup(c1.0, size) });
    let r2 = r1.try_clone().unwrap();
    drop((c1, r1));
    assert_eq!(rust_bytes(&r2, r2.as_c_ptr(), 1), c2.bytes(size));
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn ownership_crosses_both_ways_between_av_free_and_drop() {
    // A Rust-built array released by C.
    let r = alloc_array_from_fn(16, |i| i as u8).unwrap();
    let (ptr, len) = r.into_raw_parts();
    assert_eq!(len, 16);
    // SAFETY: `ptr` is the live `av_malloc_array` block `r` gave up.
    unsafe { ffi::av_free(ptr.cast()) };

    // A C-built array released by Rust.
    // SAFETY: `av_calloc` has no preconditions.
    let p = unsafe { ffi::av_calloc(16, 4) }.cast::<u32>();
    // SAFETY: `p` holds 16 zeroed `u32`s from `av_calloc`, owned from here.
    let r = unsafe { HeapVec::<u32>::from_raw_parts(p, 16) }.unwrap();
    assert!(r.as_slice().iter().all(|&x| x == 0));

    // And one object each way.
    let b = HeapBox::<Rational>::new_zeroed().unwrap();
    // SAFETY: the box's live `av_mallocz` block, given up to C.
    unsafe { ffi::av_free(b.into_c().cast()) };
}

/// A C string, freed with `av_free` on drop.
struct CString(*mut c_char);

impl CString {
    /// The bytes before the NUL, or `None` for NULL.
    fn bytes(&self) -> Option<&[u8]> {
        // SAFETY: `self.0` is NULL or a live NUL-terminated string we own,
        // which the borrow keeps alive.
        (!self.0.is_null()).then(|| unsafe { CStr::from_ptr(self.0) }.to_bytes())
    }
}

impl Drop for CString {
    fn drop(&mut self) {
        // SAFETY: `self.0` is NULL or a live `av_malloc`-family string we own.
        unsafe { ffi::av_free(self.0.cast()) }
    }
}

/// Byte strings with and without interior NULs, every non-NUL byte value, and
/// a 4 KiB run; views stay at or under 4 KiB for `BSan`.
fn string_samples() -> Vec<Vec<u8>> {
    vec![
        Vec::new(),
        b"\0".to_vec(),
        b"a".to_vec(),
        b"crustify".to_vec(),
        b"ab\0cd".to_vec(),
        b"trailing\0".to_vec(),
        (1..=255u8).collect(),
        vec![b'z'; 4096],
    ]
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn string_from_c_str_matches_av_strdup() {
    for sample in string_samples() {
        let src = CStr::from_bytes_until_nul(&sample).map_or_else(
            |_| std::ffi::CString::new(sample.clone()).unwrap(),
            CStr::to_owned,
        );
        // SAFETY: `src` is a live NUL-terminated string, only read.
        let c = CString(unsafe { ffi::av_strdup(src.as_ptr()) });
        let r = string_from_c_str(&src);
        assert_eq!(r.as_ref().map(HeapString::as_bytes), c.bytes(), "{src:?}");
        let r = r.unwrap();
        assert_ne!(r.as_ptr(), src.as_ptr());
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn string_from_bytes_until_nul_matches_av_strndup_at_every_length() {
    for sample in string_samples() {
        // Every length up to and including a terminating NUL; the ends only
        // of the 4 KiB run.
        let mut padded = sample.clone();
        padded.push(0);
        let max = padded.len();
        for len in (0..=max).filter(|&l| l < 16 || l + 16 > max) {
            // SAFETY: `padded` is `max` readable bytes, and `av_strndup` reads
            // at most `len <= max` of them.
            let c = CString(unsafe { ffi::av_strndup(padded.as_ptr().cast(), len) });
            let r = string_from_bytes_until_nul(&padded[..len]);
            assert_eq!(
                r.as_ref().map(HeapString::as_bytes),
                c.bytes(),
                "len = {len} of {sample:?}"
            );
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn string_prefixes_of_a_c_string_match_av_strndup() {
    let src = c"0123456789";
    for n in 0..=src.count_bytes() + 3 {
        // SAFETY: `src` is NUL-terminated, and `av_strndup` stops at the NUL
        // or after `n` bytes, whichever comes first.
        let c = CString(unsafe { ffi::av_strndup(src.as_ptr(), n) });
        let bytes = src.to_bytes();
        let r = string_from_bytes_until_nul(&bytes[..n.min(bytes.len())]).unwrap();
        assert_eq!(Some(r.as_bytes()), c.bytes(), "n = {n}");
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn string_clone_matches_av_strdup_step_by_step() {
    // Step 1: one source each.
    // SAFETY: a literal is a live NUL-terminated string.
    let c0 = CString(unsafe { ffi::av_strdup(c"step".as_ptr()) });
    let r0 = string_from_c_str(c"step").unwrap();
    assert_eq!(Some(r0.as_bytes()), c0.bytes());

    // Step 2: one copy each.
    // SAFETY: `c0.0` is a live string.
    let c1 = CString(unsafe { ffi::av_strdup(c0.0) });
    let r1 = r0.clone();
    assert_eq!(Some(r1.as_bytes()), c1.bytes());
    assert_ne!(r1.as_ptr(), r0.as_ptr());

    // Step 3: the C source is overwritten in place (the Rust owner exposes
    // no write path); the copies keep their bytes.
    // SAFETY: `c0.0` holds 5 writable bytes.
    unsafe { c0.0.cast::<u8>().write(b'S') };
    assert_eq!(c0.bytes(), Some(&b"Step"[..]));
    assert_eq!(Some(r1.as_bytes()), c1.bytes());

    // Step 4: copy the copies after the sources are gone.
    drop((c0, r0));
    // SAFETY: `c1.0` is a live string.
    let c2 = CString(unsafe { ffi::av_strdup(c1.0) });
    let r2 = r1.try_clone().unwrap();
    drop((c1, r1));
    assert_eq!(Some(r2.as_bytes()), c2.bytes());
    assert_eq!(r2.to_str(), Ok("step"));
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn null_sources_are_c_only() {
    // C maps a NULL source to NULL; the safe API takes no NULL, so there is
    // nothing to compare on the Rust side.
    // SAFETY: NULL is accepted and yields NULL.
    assert!(unsafe { ffi::av_strdup(core::ptr::null()) }.is_null());
    // SAFETY: as above.
    assert!(unsafe { ffi::av_strndup(core::ptr::null(), 4) }.is_null());
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn string_ownership_crosses_both_ways_between_av_free_and_drop() {
    // A Rust-built string released by C.
    let r = string_from_bytes_until_nul(b"to C").unwrap();
    let p = r.into_raw();
    // SAFETY: `p` is the live `av_strndup` string `r` gave up.
    assert_eq!(unsafe { CStr::from_ptr(p) }, c"to C");
    // SAFETY: as above, released once here.
    unsafe { ffi::av_free(p.cast()) };

    // A C-built string released by Rust.
    // SAFETY: a literal is a live NUL-terminated string.
    let p = unsafe { ffi::av_strdup(c"to Rust".as_ptr()) };
    // SAFETY: `p` is a fresh `av_strdup` string, owned from here.
    let r = unsafe { HeapString::from_raw(p) }.unwrap();
    assert_eq!(r.to_str(), Ok("to Rust"));
}

/// Set in the child process an isolated test re-runs itself in.
const ISOLATED: &str = "CRUSTIFY_EQUIV_ISOLATED";

/// The `av_max_alloc` limit is process-global, so this test lowers it only in
/// a child process running nothing else, and passes when the child does.
#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn string_failure_at_the_av_max_alloc_limit_matches_c() {
    if std::env::var_os(ISOLATED).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "mem::string_failure_at_the_av_max_alloc_limit_matches_c",
            ])
            .args(["--test-threads=1", "--nocapture"])
            .env(ISOLATED, "1")
            .status()
            .unwrap();
        assert!(status.success(), "isolated run failed: {status}");
        return;
    }

    // A copy of `n` bytes takes `n + 1`, so 7 bytes fit an 8-byte limit and
    // 8 do not.
    const LIMIT: usize = 8;
    // Built before the limit drops.
    let fits = string_from_c_str(c"0123456").unwrap();
    let over = string_from_c_str(c"01234567").unwrap();
    // SAFETY: `av_max_alloc` takes a size by value; nothing else runs in this
    // process.
    unsafe { ffi::av_max_alloc(LIMIT) };

    for src in [c"", c"0123456", c"01234567", c"012345678"] {
        // SAFETY: `src` is a live NUL-terminated string, only read.
        let c = CString(unsafe { ffi::av_strdup(src.as_ptr()) });
        let r = string_from_c_str(src);
        assert_eq!(r.as_ref().map(HeapString::as_bytes), c.bytes(), "{src:?}");
        assert_eq!(r.is_some(), src.count_bytes() < LIMIT, "{src:?}");
    }

    // `av_strndup` sizes its copy by the bytes before the first NUL, not by
    // `len`: a long buffer with an early NUL still fits.
    let padded = b"0123\x0056789abcdef";
    let long = b"0123456789abcdef";
    for bytes in [&padded[..], &long[..]] {
        for len in 0..=bytes.len() {
            // SAFETY: `bytes` is `bytes.len()` readable bytes, and
            // `av_strndup` reads at most `len` of them.
            let c = CString(unsafe { ffi::av_strndup(bytes.as_ptr().cast(), len) });
            let r = string_from_bytes_until_nul(&bytes[..len]);
            assert_eq!(
                r.as_ref().map(HeapString::as_bytes),
                c.bytes(),
                "len = {len} of {bytes:?}"
            );
        }
    }

    // Clones fail where `av_strdup` of the same source does.
    for owner in [&fits, &over] {
        // SAFETY: `owner` keeps its live string alive for the call.
        let c = CString(unsafe { ffi::av_strdup(owner.as_ptr()) });
        let r = owner.try_clone();
        assert_eq!(r.as_ref().map(HeapString::as_bytes), c.bytes());
    }
    assert!(over.try_clone().is_none());

    // SAFETY: as above; the default limit again.
    unsafe { ffi::av_max_alloc(i32::MAX as usize) };
    assert_eq!(over.clone().as_bytes(), b"01234567");
}

/// A C `av_dynarray` pair, released with `av_freep` on drop.
struct CDyn<T> {
    tab: *mut T,
    nb: c_int,
}

impl<T: Copy> CDyn<T> {
    fn new() -> Self {
        Self {
            tab: core::ptr::null_mut(),
            nb: 0,
        }
    }

    /// The elements' bytes.
    fn bytes(&self) -> &[u8] {
        if self.tab.is_null() {
            return &[];
        }
        // SAFETY: `tab` holds `nb` initialised elements, owned by `self`.
        unsafe { core::slice::from_raw_parts(self.tab.cast(), self.nb as usize * size_of::<T>()) }
    }

    /// `av_dynarray2_add` with a copy of `v`; whether it returned a slot.
    fn add2(&mut self, v: T) -> bool {
        // SAFETY: `tab` / `nb` were grown only by this family from NULL / 0;
        // `v` is `size_of::<T>()` readable bytes.
        let slot = unsafe {
            ffi::av_dynarray2_add(
                (&raw mut self.tab).cast(),
                &raw mut self.nb,
                size_of::<T>(),
                (&raw const v).cast(),
            )
        };
        !slot.is_null()
    }

    /// `av_dynarray_add` of `p`.
    fn add(&mut self, p: *mut c_void) {
        // SAFETY: as `add2`, for `void *` elements (`T` is pointer-sized).
        unsafe { ffi::av_dynarray_add((&raw mut self.tab).cast(), &raw mut self.nb, p) }
    }

    /// `av_dynarray_add_nofree` of `p`.
    fn add_nofree(&mut self, p: *mut c_void) -> c_int {
        // SAFETY: as `add`.
        unsafe { ffi::av_dynarray_add_nofree((&raw mut self.tab).cast(), &raw mut self.nb, p) }
    }
}

impl<T> Drop for CDyn<T> {
    fn drop(&mut self) {
        // SAFETY: `tab` is NULL or an `av_realloc` block we own.
        unsafe { ffi::av_freep((&raw mut self.tab).cast()) }
    }
}

/// The bytes of a `DynArray` of plain elements.
fn dyn_bytes<T: ffibox::CPlainElem>(a: &DynArray<T>) -> &[u8] {
    let s = a.as_slice();
    rust_bytes(a, s.as_ptr(), s.len())
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn value_pushes_match_av_dynarray2_add_after_every_step() {
    fn check<T: ffibox::CPlainElem + Copy>(make: impl Fn(usize) -> T) {
        let (mut r, mut c) = (DynArray::<T>::new(), CDyn::<T>::new());
        // Up to 4 KiB of elements, across every doubling up to 512.
        for i in 0..(4096 / size_of::<T>()).min(600) {
            let v = make(i);
            assert_eq!(r.push_or_clear(v), c.add2(v), "push {i}");
            assert_eq!(r.len(), c.nb as usize, "push {i}");
            assert_eq!(dyn_bytes(&r), c.bytes(), "push {i}");
        }
    }
    check(|i| i as u8);
    check(|i| i as u32 ^ 0xDEAD_BEEF);
    check(|i| [i as u8, !i as u8, 7]);
    check(|i| (i as f64) * -1.5);
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn cell_value_pushes_match_av_dynarray2_add() {
    let (mut r, mut c) = (DynArray::<Rational>::new(), CDyn::<ffi::AVRational>::new());
    for i in 0..70 {
        let v = ffi::AVRational {
            num: i,
            den: -i - 1,
        };
        assert!(r.push_or_clear(Rational(v, PhantomData)));
        assert!(c.add2(v));
        let h = r.as_handles();
        assert_eq!(h.len(), c.nb as usize);
        assert_eq!(rust_bytes(&r, h.as_ptr(), h.len()), c.bytes(), "push {i}");
    }
}

/// The elements of a C array.
fn c_elems<T: Copy>(c: &CDyn<T>) -> &[T] {
    if c.tab.is_null() {
        return &[];
    }
    // SAFETY: `c.tab` holds `c.nb` initialised `T`, owned by `c`.
    unsafe { core::slice::from_raw_parts(c.tab, c.nb as usize) }
}

/// C's `av_dynarray_add_nofree` status as the Rust result.
fn status(k: c_int) -> Result<(), c_int> {
    if k < 0 { Err(k) } else { Ok(()) }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn pointer_pushes_match_av_dynarray_add_after_every_step() {
    let targets = [0u16; 40];
    let (mut r, mut c) = (DynArray::<*const u16>::new(), CDyn::<*const u16>::new());
    let (mut rn, mut cn) = (DynArray::<*mut u16>::new(), CDyn::<*mut u16>::new());
    for (i, t) in targets.iter().enumerate() {
        let p: *const u16 = t;
        assert!(r.push_ptr_or_clear(p));
        c.add(p.cast_mut().cast());
        assert_eq!(r.as_slice(), c_elems(&c), "push {i}");

        // Stored as given, never followed: past-the-end pointers too.
        let q = p.cast_mut().wrapping_add(i);
        let k = cn.add_nofree(q.cast());
        assert_eq!(rn.try_push_ptr(q), status(k), "push {i}");
        assert_eq!(rn.as_slice(), c_elems(&cn), "push {i}");
    }
    assert!(r.push_ptr_or_clear(core::ptr::null()));
    c.add(core::ptr::null_mut());
    assert_eq!(r.as_slice(), c_elems(&c));
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn dynarray_ownership_moves_into_a_heap_vec() {
    let mut r = DynArray::<u64>::new();
    for i in 0..5 {
        assert!(r.push_or_clear(i * 3));
    }
    let v = r.into_vec();
    assert_eq!(v.as_slice(), &[0, 3, 6, 9, 12]);
    // C frees what the array grew.
    let (ptr, len) = v.into_raw_parts();
    assert_eq!(len, 5);
    // SAFETY: `ptr` is the `av_realloc` block the array owned, now ours.
    unsafe { ffi::av_free(ptr.cast()) };
    assert!(DynArray::<u64>::new().into_vec().as_ptr().is_null());
}

/// The `av_max_alloc` limit is process-global, so this test lowers it only in
/// a child process running nothing else, and passes when the child does.
#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn dynarray_failure_at_the_av_max_alloc_limit_matches_c() {
    if std::env::var_os(ISOLATED).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "mem::dynarray_failure_at_the_av_max_alloc_limit_matches_c",
            ])
            .args(["--test-threads=1", "--nocapture"])
            .env(ISOLATED, "1")
            .status()
            .unwrap();
        assert!(status.success(), "isolated run failed: {status}");
        return;
    }

    // 64 bytes hold 8 pointers or `u64`: the doubling to 16 fails, on the
    // ninth append.
    // SAFETY: `av_max_alloc` takes a size by value; nothing else runs in this
    // process.
    unsafe { ffi::av_max_alloc(64) };

    let (mut r, mut c) = (DynArray::<u64>::new(), CDyn::<u64>::new());
    for i in 0..12u64 {
        assert_eq!(r.push_or_clear(i), c.add2(i), "value push {i}");
        assert_eq!(
            (r.len(), dyn_bytes(&r)),
            (c.nb as usize, c.bytes()),
            "value push {i}"
        );
        assert!(c.nb > 0 || c.tab.is_null());
    }
    assert_eq!(r.len(), 3, "emptied on the ninth append, then regrown");

    let x = 0u8;
    let p: *const u8 = &raw const x;
    let (mut r, mut c) = (DynArray::<*const u8>::new(), CDyn::<*const u8>::new());
    for i in 0..12 {
        let ok = r.push_ptr_or_clear(p);
        c.add(p.cast_mut().cast());
        assert_eq!(ok, c.nb != 0, "push {i}");
        assert_eq!(r.as_slice(), c_elems(&c), "push {i}");
    }

    let (mut r, mut c) = (DynArray::<*const u8>::new(), CDyn::<*const u8>::new());
    for i in 0..12 {
        let k = c.add_nofree(p.cast_mut().cast());
        assert_eq!(r.try_push_ptr(p), status(k), "push {i}");
        assert_eq!(r.as_slice(), c_elems(&c), "push {i}");
    }
    assert_eq!(r.len(), 8, "a failed nofree append keeps the array");

    // SAFETY: as above; the default limit again.
    unsafe { ffi::av_max_alloc(i32::MAX as usize) };
}

/// A C `av_fast` pair, released with `av_free` on drop.
struct CFast {
    ptr: *mut u8,
    size: c_uint,
}

impl CFast {
    fn new() -> Self {
        Self {
            ptr: core::ptr::null_mut(),
            size: 0,
        }
    }

    /// The whole buffer, which the caller has initialised.
    fn bytes(&self) -> &[u8] {
        if self.ptr.is_null() {
            return &[];
        }
        // SAFETY: `ptr` is a live block of `size` bytes we own, initialised
        // per each caller.
        unsafe { core::slice::from_raw_parts(self.ptr, self.size as usize) }
    }

    /// Fills the whole buffer with `pattern(size, seed)`.
    fn fill(&mut self, seed: u8) {
        if !self.ptr.is_null() {
            // SAFETY: `ptr` is a live, writable block of `size` bytes we own.
            let b = unsafe { core::slice::from_raw_parts_mut(self.ptr, self.size as usize) };
            b.copy_from_slice(&pattern(b.len(), seed));
        }
    }
}

impl Drop for CFast {
    fn drop(&mut self) {
        // SAFETY: `ptr` is NULL or a block we own.
        unsafe { ffi::av_free(self.ptr.cast()) }
    }
}

/// `len` bytes `seed`, `seed + 1`, ..., written into buffers in one copy:
/// `BSan` retags per reference, so a byte-at-a-time fill of a long-lived
/// buffer takes minutes there.
fn pattern(len: usize, seed: u8) -> Vec<u8> {
    (0..len).map(|i| seed.wrapping_add(i as u8)).collect()
}

/// [`pattern`] as initialised `MaybeUninit` bytes.
fn pattern_uninit(len: usize, seed: u8) -> Vec<MaybeUninit<u8>> {
    pattern(len, seed)
        .into_iter()
        .map(MaybeUninit::new)
        .collect()
}

/// Requested sizes around every branch: zero, growth, reuse at and below the
/// current size, and requests above `UINT_MAX` that must fail. Successes stay
/// under 4 KiB, for `BSan`.
const FAST_SIZES: [usize; 13] = [
    0,
    1,
    10,
    5,
    100,
    164,
    165,
    3000,
    50,
    0,
    u32::MAX as usize + 1,
    2,
    usize::MAX,
];

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn reserve_matches_av_fast_realloc_after_every_step() {
    let (mut r, mut c) = (FastBuffer::new(), CFast::new());
    for (step, min) in FAST_SIZES.into_iter().enumerate() {
        // Fill both sides alike, so the bytes a reallocation keeps compare.
        let old = r.capacity();
        let fill = pattern_uninit(r.capacity(), step as u8);
        r.as_mut_slice().copy_from_slice(&fill);
        c.fill(step as u8);
        let kept = c.bytes().to_vec();

        let mut size = c.size;
        // SAFETY: `c.ptr` is NULL or a block of `c.size` bytes we own.
        let new = unsafe { ffi::av_fast_realloc(c.ptr.cast(), &raw mut size, min) };
        let ok = r.reserve(min);
        assert_eq!(
            ok,
            !new.is_null() || min <= c.size as usize,
            "step {step}: {min}"
        );
        if ok {
            (c.ptr, c.size) = (new.cast(), size);
            assert_eq!(r.capacity(), c.size as usize, "step {step}: {min}");
        } else {
            // C zeroes the size and leaves the old block to its caller; the
            // buffer keeps it, with its size.
            assert_eq!((new, size), (core::ptr::null_mut(), 0), "step {step}");
            assert_eq!(r.capacity(), old, "step {step}");
        }
        // The old bytes survive on both sides.
        // SAFETY: the prefix of `old` bytes was written above, and moved.
        let rb = unsafe { core::slice::from_raw_parts(r.as_slice().as_ptr().cast::<u8>(), old) };
        assert_eq!(rb, &kept[..old.min(kept.len())], "step {step}");
        assert_eq!(
            &c.bytes()[..old.min(c.bytes().len())],
            &kept[..old.min(kept.len())]
        );
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn reserve_discarding_matches_av_fast_malloc_after_every_step() {
    let (mut r, mut c) = (FastBuffer::new(), CFast::new());
    for (step, min) in FAST_SIZES.into_iter().enumerate() {
        let fill = pattern_uninit(r.capacity(), step as u8);
        r.as_mut_slice().copy_from_slice(&fill);
        c.fill(step as u8);
        let before = c.bytes().to_vec();

        let mut slot = c.ptr.cast::<c_void>();
        // SAFETY: the slot holds NULL or a block of `c.size` bytes we own.
        unsafe { ffi::av_fast_malloc((&raw mut slot).cast(), &raw mut c.size, min) };
        c.ptr = slot.cast();
        let ok = r.reserve_discarding(min);
        assert_eq!(ok, !c.ptr.is_null() || min == 0, "step {step}: {min}");
        assert_eq!(r.capacity(), c.size as usize, "step {step}: {min}");
        if c.size as usize == before.len() && !before.is_empty() {
            // Reused: the contents stay, alike.
            // SAFETY: every byte was written above.
            let rb = unsafe {
                core::slice::from_raw_parts(r.as_slice().as_ptr().cast::<u8>(), r.capacity())
            };
            assert_eq!((rb, c.bytes()), (&before[..], &before[..]), "step {step}");
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn zeroed_reserve_matches_av_fast_mallocz_after_every_step() {
    let (mut r, mut c) = (ZeroedFastBuffer::new(), CFast::new());
    for (step, min) in FAST_SIZES.into_iter().enumerate() {
        let mut slot = c.ptr.cast::<c_void>();
        // SAFETY: the slot holds NULL or a block of `c.size` bytes we own.
        unsafe { ffi::av_fast_mallocz((&raw mut slot).cast(), &raw mut c.size, min) };
        c.ptr = slot.cast();
        let ok = r.reserve_discarding(min);
        assert_eq!(ok, !c.ptr.is_null() || min == 0, "step {step}: {min}");
        assert_eq!(r.capacity(), c.size as usize, "step {step}: {min}");
        // Fresh blocks are zero and reused ones keep the last fill, alike.
        assert_eq!(r.as_slice(), c.bytes(), "step {step}: {min}");
        let fill = pattern(r.capacity(), step as u8);
        r.as_mut_slice().copy_from_slice(&fill);
        c.fill(step as u8);
    }
}

/// The `av_fast` family's other failure: `realloc` or `av_malloc` refusing a
/// request within the `av_max_alloc` limit. Only an exhausted address space
/// reaches it, so this test re-runs itself in a child process under a 512 MiB
/// `ulimit -v` and asks for 1 GiB; it passes when the child does. The
/// sanitizer runtimes reserve shadow memory far beyond any such limit and
/// abort on a refused allocation, so those variants skip it.
#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn fast_buffer_failure_without_address_space_matches_c() {
    const NAME: &str = "mem::fast_buffer_failure_without_address_space_matches_c";
    if std::env::var_os(ISOLATED).is_none() {
        if !cfg!(unix)
            || matches!(
                std::env::var("CRUSTIFY_BUILD_VARIANT").as_deref(),
                Ok("asan-ubsan" | "tsan" | "bsan")
            )
        {
            return;
        }
        let exe = std::env::current_exe().unwrap();
        let status = std::process::Command::new("sh")
            .arg("-c")
            .arg("ulimit -v 524288 && exec \"$0\" \"$@\"")
            .arg(exe)
            .args(["--exact", NAME, "--test-threads=1", "--nocapture"])
            .env(ISOLATED, "1")
            .status()
            .unwrap();
        assert!(status.success(), "isolated run failed: {status}");
        return;
    }

    const HUGE: usize = 1 << 30;
    let (mut r, mut c) = (FastBuffer::new(), CFast::new());
    assert!(r.reserve(100));
    let fill = pattern_uninit(r.capacity(), 3);
    r.as_mut_slice().copy_from_slice(&fill);
    let mut size = 0;
    // SAFETY: `av_fast_realloc` takes NULL with a size of 0.
    c.ptr = unsafe { ffi::av_fast_realloc(core::ptr::null_mut(), &raw mut size, 100) }.cast();
    c.size = size;
    c.fill(3);
    let kept = c.bytes().to_vec();
    assert_eq!(r.capacity(), kept.len());

    // `realloc` fails: C reports a size of 0 and leaves the block to its
    // caller; the buffer keeps it, with its size and bytes.
    let mut size = c.size;
    // SAFETY: `c.ptr` is a block of `c.size` bytes we own; on failure it is
    // still ours.
    let new = unsafe { ffi::av_fast_realloc(c.ptr.cast(), &raw mut size, HUGE) };
    assert_eq!((new, size), (core::ptr::null_mut(), 0));
    assert!(!r.reserve(HUGE));
    assert_eq!(r.capacity(), kept.len());
    // SAFETY: the first `kept.len()` bytes were written above and kept.
    let rb = unsafe { core::slice::from_raw_parts(r.as_slice().as_ptr().cast::<u8>(), kept.len()) };
    assert_eq!((rb, c.bytes()), (&kept[..], &kept[..]));
    // Both go on to grow normally.
    size = c.size;
    // SAFETY: as above.
    let new = unsafe { ffi::av_fast_realloc(c.ptr.cast(), &raw mut size, 200) };
    assert!(!new.is_null());
    (c.ptr, c.size) = (new.cast(), size);
    assert!(r.reserve(200));
    assert_eq!(r.capacity(), c.size as usize);

    // `av_malloc` fails: both sides free the old block and end up empty.
    for zeroed in [false, true] {
        let mut slot = c.ptr.cast::<c_void>();
        if zeroed {
            // SAFETY: the slot holds a block of `c.size` bytes we own.
            unsafe { ffi::av_fast_mallocz((&raw mut slot).cast(), &raw mut c.size, HUGE) };
        } else {
            // SAFETY: as above.
            unsafe { ffi::av_fast_malloc((&raw mut slot).cast(), &raw mut c.size, HUGE) };
        }
        c.ptr = slot.cast();
        assert_eq!((c.ptr, c.size), (core::ptr::null_mut(), 0));
        if zeroed {
            let mut z = ZeroedFastBuffer::new();
            assert!(z.reserve_discarding(50));
            assert!(!z.reserve_discarding(HUGE));
            assert_eq!((z.capacity(), z.as_slice()), (0, &[][..]));
            assert!(z.reserve_discarding(60));
            assert_eq!(z.as_slice(), &[0; 60 + 60 / 16 + 32][..]);
        } else {
            assert!(!r.reserve_discarding(HUGE));
            assert_eq!(r.capacity(), 0);
            assert!(r.reserve_discarding(60));
            assert_eq!(r.capacity(), 60 + 60 / 16 + 32);
        }
        // SAFETY: NULL with a size of 0 is an empty pair.
        unsafe { ffi::av_fast_malloc((&raw mut slot).cast(), &raw mut c.size, 60) };
        c.ptr = slot.cast();
        assert_eq!(c.size as usize, 60 + 60 / 16 + 32);
    }
}

// ---- the av_realloc family ----

mod realloc {
    use core::ffi::c_void;
    use core::mem::{MaybeUninit, size_of};

    use ffmpeg_bindings::libavutil::libavutil::ffi;
    use ffmpeg_bindings::libavutil::libavutil::mem::{
        HeapVec, alloc_array, alloc_array_from_fn, checked_size_mul, realloc_array,
        realloc_array_in_place, realloc_array_or_free, realloc_bytes, realloc_bytes_in_place,
        resize_array_with,
    };

    use super::CBlock;

    /// Sizes around the `av_max_alloc` limit (`INT_MAX`) and overflow, with
    /// successes of at most 4 KiB.
    const LENS: [usize; 9] = [
        0,
        1,
        5,
        64,
        4096,
        i32::MAX as usize + 1,
        usize::MAX / 2 + 1,
        usize::MAX - 1,
        usize::MAX,
    ];

    /// `len` bytes counting up from `seed`, in an `av_malloc` block on each
    /// side.
    fn filled(len: usize, seed: u8) -> (CBlock, HeapVec<MaybeUninit<u8>>) {
        // SAFETY: `av_malloc` has no preconditions.
        let c = CBlock(unsafe { ffi::av_malloc(len) });
        let mut r = alloc_array::<u8>(len).unwrap();
        for (i, b) in r.as_mut_slice().iter_mut().enumerate() {
            b.write(seed.wrapping_add(i as u8));
        }
        // SAFETY: `c` holds `len` writable bytes, filled like `r`.
        let cb = unsafe { core::slice::from_raw_parts_mut(c.0.cast::<u8>(), len) };
        for (i, b) in cb.iter_mut().enumerate() {
            *b = seed.wrapping_add(i as u8);
        }
        (c, r)
    }

    /// The first `n` bytes of `r`, which must be initialised.
    fn prefix(r: &HeapVec<MaybeUninit<u8>>, n: usize) -> Vec<u8> {
        // SAFETY: per each caller, the first `n` bytes were written.
        r.as_slice()[..n]
            .iter()
            .map(|b| unsafe { b.assume_init() })
            .collect()
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn realloc_bytes_matches_av_realloc_step_by_step() {
        let (mut c, mut r) = filled(16, 3);
        let mut kept = 16;
        for size in [32, 8, 0, 1, 4000, i32::MAX as usize + 1, 100, usize::MAX] {
            // SAFETY: `c.0` is a live block we own, which `av_realloc`
            // releases on success and leaves alone on failure.
            let cp = unsafe { ffi::av_realloc(c.0, size) };
            let rr = realloc_bytes(r, size);
            assert_eq!(rr.is_ok(), !cp.is_null(), "size {size}");
            r = match rr {
                Ok(r) => {
                    c.0 = cp;
                    kept = kept.min(size);
                    assert_eq!(r.len(), size);
                    r
                }
                Err(r) => r,
            };
            // The surviving bytes agree on both sides after every step.
            assert_eq!(prefix(&r, kept), c.bytes(kept), "size {size}");
            assert_eq!(c.0 as usize % align_of::<ffi::max_align_t>(), 0);
            assert_eq!(r.as_ptr() as usize % align_of::<ffi::max_align_t>(), 0);
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn realloc_array_matches_av_realloc_array() {
        fn check<T>() {
            for n in LENS {
                // SAFETY: `av_realloc_array(NULL, ...)` allocates.
                let c = CBlock(unsafe {
                    ffi::av_realloc_array(core::ptr::null_mut(), n, size_of::<T>())
                });
                let r = realloc_array::<T>(HeapVec::empty(), n);
                assert_eq!(
                    r.is_ok(),
                    !c.0.is_null(),
                    "n = {n}, size = {}",
                    size_of::<T>()
                );
                if let Ok(r) = r {
                    assert_eq!(r.len(), n);
                    assert!(!r.as_ptr().is_null());
                } else {
                    assert!(r.unwrap_err().is_empty());
                }
            }
        }
        check::<u8>();
        check::<u32>();
        check::<[u64; 3]>();
        check::<()>();
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn resize_array_with_keeps_what_av_realloc_array_keeps() {
        let mut r = alloc_array_from_fn(4, |i| i as u32 * 10).unwrap();
        // SAFETY: `av_malloc_array` has no preconditions.
        let mut c = CBlock(unsafe { ffi::av_malloc_array(4, 4) });
        // SAFETY: `c` holds 4 writable `u32`.
        unsafe { c.0.cast::<[u32; 4]>().write([0, 10, 20, 30]) };
        let mut len = 4;
        for n in [6, 2, 2, 9, usize::MAX / 4, 1] {
            // SAFETY: `c.0` is a live block we own.
            let cp = unsafe { ffi::av_realloc_array(c.0, n, 4) };
            let rr = resize_array_with(r, n, |i| i as u32 * 10);
            assert_eq!(rr.is_ok(), !cp.is_null(), "n = {n}");
            r = match rr {
                Ok(r) => {
                    c.0 = cp;
                    // C's added tail is uninitialised; write it as `f` does.
                    for i in len.min(n)..n {
                        // SAFETY: as the comment below states, for this part of the operation.
                        let v_3 = unsafe { c.0.cast::<u32>().add(i) };
                        // SAFETY: `c` holds `n` writable `u32`.
                        unsafe { v_3.write(i as u32 * 10) };
                    }
                    len = n;
                    r
                }
                Err(r) => r,
            };
            assert_eq!(r.len(), len);
            let bytes = c.bytes(len * 4);
            let cvals: Vec<u32> = bytes
                .chunks(4)
                .map(|b| u32::from_ne_bytes(b.try_into().unwrap()))
                .collect();
            assert_eq!(r.as_slice(), &cvals[..], "n = {n}");
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn realloc_array_or_free_matches_av_realloc_f() {
        for n in LENS {
            let (c, r) = filled(8, 1);
            let c = core::mem::ManuallyDrop::new(c);
            // SAFETY: `c.0` is a live block whose ownership passes to C,
            // which frees it on failure.
            let cp = CBlock(unsafe { ffi::av_realloc_f(c.0, n, 2) });
            let rr = realloc_array_or_free(
                // The same 8 bytes as 4 two-byte elements.
                {
                    let (p, _) = r.into_raw_parts();
                    // SAFETY: the 8-byte `av_malloc` block holds 4 `u16`.
                    unsafe { HeapVec::<MaybeUninit<u16>>::from_raw_parts(p.cast(), 4) }.unwrap()
                },
                n,
            );
            assert_eq!(rr.is_some(), !cp.0.is_null(), "n = {n}");
            if let Some(rr) = rr {
                assert_eq!(rr.len(), n);
                let kept = (4 * 2).min(n * 2);
                let rb: Vec<u8> = rr.as_slice()[..kept / 2]
                    .iter()
                    // SAFETY: the first `kept` bytes carried over.
                    .flat_map(|v| unsafe { v.assume_init() }.to_ne_bytes())
                    .collect();
                assert_eq!(rb, cp.bytes(kept), "n = {n}");
            }
        }
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn realloc_bytes_in_place_matches_av_reallocp() {
        let mut cslot: *mut c_void = core::ptr::null_mut();
        let mut r = HeapVec::<MaybeUninit<u8>>::empty();
        let mut kept = 0usize;
        for size in [8, 24, 0, 0, 3, i32::MAX as usize + 1, 5, usize::MAX, 2] {
            // SAFETY: `cslot` is a live slot holding NULL or a block we own.
            let cret = unsafe { ffi::av_reallocp((&raw mut cslot).cast(), size) };
            let rret = realloc_bytes_in_place(&mut r, size);
            assert_eq!(rret.err().unwrap_or(0), cret, "size {size}");
            assert_eq!(r.as_ptr().is_null(), cslot.is_null(), "size {size}");
            let len = if cslot.is_null() { 0 } else { size };
            assert_eq!(r.len(), len, "size {size}");
            // Fill the new tail identically, then compare everything.
            kept = kept.min(len);
            for i in kept..len {
                r.as_mut_slice()[i].write(i as u8 ^ 0x5A);
                // SAFETY: as the comment below states, for this part of the operation.
                let v_2 = unsafe { cslot.cast::<u8>().add(i) };
                // SAFETY: `cslot` holds `len` writable bytes.
                unsafe { v_2.write(i as u8 ^ 0x5A) };
            }
            kept = len;
            if len > 0 {
                let c = core::mem::ManuallyDrop::new(CBlock(cslot));
                assert_eq!(prefix(&r, len), c.bytes(len));
            }
        }
        drop(CBlock(cslot));
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn realloc_array_in_place_matches_av_reallocp_array() {
        fn check<T>() {
            let mut cslot: *mut c_void = core::ptr::null_mut();
            let mut r = HeapVec::<MaybeUninit<T>>::empty();
            for n in [3, 0, 7, i32::MAX as usize, 2, usize::MAX, 1, 0] {
                // SAFETY: `cslot` is a live slot holding NULL or a block we
                // own.
                let cret =
                    unsafe { ffi::av_reallocp_array((&raw mut cslot).cast(), n, size_of::<T>()) };
                let rret = realloc_array_in_place(&mut r, n);
                assert_eq!(
                    rret.err().unwrap_or(0),
                    cret,
                    "n = {n}, size = {}",
                    size_of::<T>()
                );
                assert_eq!(r.as_ptr().is_null(), cslot.is_null(), "n = {n}");
                assert_eq!(r.len(), if cslot.is_null() { 0 } else { n });
            }
            drop(CBlock(cslot));
        }
        check::<u8>();
        check::<u64>();
        check::<()>();
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn size_mult_matches_av_size_mult() {
        let v = [
            0,
            1,
            2,
            3,
            1 << 31,
            1 << 32,
            (1 << 32) + 1,
            usize::MAX / 2,
            usize::MAX / 2 + 1,
            usize::MAX,
        ];
        for a in v {
            for b in v {
                let mut cr = 0xDEAD;
                // SAFETY: `cr` is a live, writable local.
                let cret = unsafe { ffi::av_size_mult(a, b, &raw mut cr) };
                assert_eq!(
                    checked_size_mul(a, b).ok_or(cret),
                    if cret == 0 { Ok(cr) } else { Err(cret) }
                );
                if cret != 0 {
                    // C leaves the out-parameter alone on overflow.
                    assert_eq!(cr, 0xDEAD);
                }
            }
        }
    }
}

/// `av_memcpy_backptr(buf + dst, back, len)` on `buf`.
fn c_backptr(buf: &mut [u8], dst: usize, back: usize, len: usize) {
    assert!(back <= dst && dst + len <= buf.len());
    // SAFETY: as the comment below states, for this part of the operation.
    let as_mut_ptr_add = unsafe { buf.as_mut_ptr().add(dst) };
    // SAFETY: `dst <= buf.len()`; C reads from `dst - back`, at or after
    // `buf[0]`, and writes `buf[dst..dst + len]`, inside `buf`.
    unsafe { ffi::av_memcpy_backptr(as_mut_ptr_add, back as c_int, len as c_int) };
}

/// A buffer of distinct bytes, so a byte copied from the wrong place shows.
fn backref_pattern(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 37 + 11) as u8).collect()
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn back_reference_matches_av_memcpy_backptr() {
    for back in 0..=40 {
        for len in (0..=70).chain([127, 128, 129, 255, 1000]) {
            for dst in [back, back + 1, 64] {
                let mut c = backref_pattern(dst + len + 3);
                let mut rust = c.clone();
                if back > dst {
                    continue;
                }
                c_backptr(&mut c, dst, back, len);
                copy_back_reference(&mut rust, dst, back, len).unwrap();
                assert_eq!(rust, c, "dst {dst} back {back} len {len}");
            }
        }
    }
}

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn chained_back_references_agree_after_every_step() {
    // An LZ77 decode: literals, then back-references into what came before.
    let steps = [(3, 9), (1, 4), (7, 20), (2, 2), (16, 33), (5, 0), (40, 64)];
    let mut c = backref_pattern(400);
    let mut rust = c.clone();
    let mut at = 8;
    for (back, len) in steps {
        c_backptr(&mut c, at, back, len);
        copy_back_reference(&mut rust, at, back, len).unwrap();
        assert_eq!(rust, c, "at {at} back {back} len {len}");
        at += len + 1;
    }
}

#[test]
fn rejected_back_references_leave_the_buffer_alone() {
    let fresh = backref_pattern(16);
    for (dst, back, len, err) in [
        (2, 3, 1, BackReferenceError::BeforeStart),
        (2, 3, 0, BackReferenceError::BeforeStart),
        (10, 1, 7, BackReferenceError::PastEnd),
        (17, 0, 0, BackReferenceError::PastEnd),
        (4, 1 << 31, 1, BackReferenceError::TooLong),
    ] {
        let mut rust = fresh.clone();
        assert_eq!(copy_back_reference(&mut rust, dst, back, len), Err(err));
        assert_eq!(rust, fresh);
    }
}

/// `set_max_alloc` moves the limit that both C and the Rust owners obey, as
/// `av_max_alloc` does. The limit is process-global, so this runs in a child
/// process running nothing else, and passes when the child does.
#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn set_max_alloc_moves_the_limit_as_av_max_alloc() {
    const NAME: &str = "mem::set_max_alloc_moves_the_limit_as_av_max_alloc";
    if std::env::var_os(ISOLATED).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", NAME, "--test-threads=1", "--nocapture"])
            .env(ISOLATED, "1")
            .status()
            .unwrap();
        assert!(status.success(), "isolated run failed: {status}");
        return;
    }

    /// Whether C and Rust each allocate `len` bytes, which must agree.
    fn fits(len: usize) -> bool {
        // SAFETY: `av_malloc` takes a size by value.
        let c = CBlock(unsafe { ffi::av_malloc(len) });
        let rust = alloc_array::<u8>(len);
        assert_eq!(rust.is_some(), !c.0.is_null(), "{len}");
        rust.is_some()
    }

    for (limit, via_rust) in [
        (64, true),
        (16, false),
        (4096, true),
        (0, false),
        (100, true),
    ] {
        if via_rust {
            set_max_alloc(limit);
        } else {
            // SAFETY: `av_max_alloc` takes a size by value; nothing else runs
            // in this process.
            unsafe { ffi::av_max_alloc(limit) };
        }
        assert!(fits(limit.min(4096)) || limit == 0);
        assert!(!fits(limit + 1));
        if limit > 0 {
            assert!(fits(limit - 1));
        }
        assert_eq!(string_from_c_str(c"ab").is_some(), limit >= 3);
    }
    set_max_alloc(i32::MAX as usize);
    assert!(fits(1 << 20));
}
