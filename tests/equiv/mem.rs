//! Equivalence of `libavutil/mem.h`'s `av_malloc`-family owners with the raw
//! C allocator, on independently owned blocks.

use core::ffi::{CStr, c_char, c_void};
use core::mem::{MaybeUninit, size_of};

use ffibox::{CZeroable, define_ctype};
use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::mem::{
    HeapBox, HeapString, HeapVec, alloc_array, alloc_array_from_fn, alloc_array_zeroed,
    alloc_cell_array_zeroed, string_from_bytes_until_nul, string_from_c_str,
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
/// under 4 KiB: an exact 2 GiB success would commit 2 GiB per side, and BSan's
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
fn alloc_array_from_fn_matches_a_filled_c_array() {
    let n = 100;
    // SAFETY: `av_malloc_array` has no preconditions.
    let c = CBlock(unsafe { ffi::av_malloc_array(n, size_of::<u32>()) });
    assert!(!c.0.is_null());
    for i in 0..n {
        // SAFETY: `c.0` holds `n` aligned `u32` slots.
        unsafe { c.0.cast::<u32>().add(i).write(i as u32 ^ 0x5a5a) };
    }
    let r = alloc_array_from_fn(n, |i| i as u32 ^ 0x5a5a).unwrap();
    assert_eq!(rust_bytes(&r, r.as_ptr(), n), c.bytes(n * 4));
}

#[test]
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
    // SAFETY: `c2.0` holds `len / 2` aligned `u16`s.
    unsafe { c2.0.cast::<u16>().add(512).write(7) };
    assert_eq!(rust_bytes(&r2, r2.as_ptr(), src.len()), c2.bytes(len));
    assert_eq!(rust_bytes(&r1, r1.as_ptr(), src.len()), c1.bytes(len));
}

#[test]
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
fn new_uninit_box_matches_av_malloc_and_fills_in_place() {
    // SAFETY: `av_malloc` has no preconditions.
    let c = CBlock(unsafe { ffi::av_malloc(size_of::<ffi::AVRational>()) });
    let mut slot = HeapBox::<Rational>::new_uninit().unwrap();
    let value = ffi::AVRational { num: -3, den: 8 };
    // SAFETY: both blocks are aligned and `AVRational`-sized, owned here.
    unsafe {
        c.0.cast::<ffi::AVRational>().write(value);
        slot.as_c_ptr().write(value);
    }
    // SAFETY: the write above initialised the whole `AVRational`.
    let r = unsafe { slot.assume_init() };
    assert_eq!(
        rust_bytes(&r, r.as_c_ptr(), 1),
        c.bytes(size_of::<ffi::AVRational>())
    );
}

#[test]
fn box_clone_matches_av_memdup_step_by_step() {
    let size = size_of::<ffi::AVRational>();
    let value = ffi::AVRational { num: 7, den: -9 };

    // Step 1: one source each, holding the same value.
    // SAFETY: `av_malloc` has no preconditions.
    let c0 = CBlock(unsafe { ffi::av_malloc(size) });
    assert!(!c0.0.is_null());
    let mut r0 = HeapBox::<Rational>::new_zeroed().unwrap();
    // SAFETY: both blocks are aligned and `AVRational`-sized, owned here.
    unsafe {
        c0.0.cast::<ffi::AVRational>().write(value);
        r0.as_mut().as_mut_ptr().write(value);
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
    // SAFETY: as in step 1.
    unsafe {
        c0.0.cast::<ffi::AVRational>().write(other);
        r0.as_mut().as_mut_ptr().write(other);
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
/// a 4 KiB run; views stay at or under 4 KiB for BSan.
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
fn string_from_c_str_matches_av_strdup() {
    for sample in string_samples() {
        let src = CStr::from_bytes_until_nul(&sample)
            .map(CStr::to_owned)
            .unwrap_or_else(|_| std::ffi::CString::new(sample.clone()).unwrap());
        // SAFETY: `src` is a live NUL-terminated string, only read.
        let c = CString(unsafe { ffi::av_strdup(src.as_ptr()) });
        let r = string_from_c_str(&src);
        assert_eq!(r.as_ref().map(HeapString::as_bytes), c.bytes(), "{src:?}");
        let r = r.unwrap();
        assert_ne!(r.as_ptr(), src.as_ptr());
    }
}

#[test]
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
fn null_sources_are_c_only() {
    // C maps a NULL source to NULL; the safe API takes no NULL, so there is
    // nothing to compare on the Rust side.
    // SAFETY: NULL is accepted and yields NULL.
    assert!(unsafe { ffi::av_strdup(core::ptr::null()) }.is_null());
    // SAFETY: as above.
    assert!(unsafe { ffi::av_strndup(core::ptr::null(), 4) }.is_null());
}

#[test]
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
