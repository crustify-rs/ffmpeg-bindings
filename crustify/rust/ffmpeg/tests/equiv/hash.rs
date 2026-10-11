//! Safe hashing and lifecycle agree with independently owned C inputs.

use core::ffi::CStr;
use core::ptr;
use ffmpeg::libavutil::{ffi, hash::HashContext};

struct Reference(*mut ffi::AVHashContext);
impl Reference {
    fn new(name: &CStr) -> Self {
        let mut context = ptr::null_mut();
        // SAFETY: valid output slot and name; C transfers the allocation.
        let result = unsafe { ffi::av_hash_alloc(&mut context, name.as_ptr()) };
        assert_eq!(result, 0);
        assert!(!context.is_null());
        // SAFETY: the fresh allocation has valid algorithm storage; initialize
        // its hashing state before any observation.
        unsafe { ffi::av_hash_init(context) };
        Self(context)
    }
}
impl Drop for Reference {
    fn drop(&mut self) {
        // SAFETY: this fixture uniquely owns the context and its children.
        unsafe { ffi::av_hash_freep(&mut self.0) };
        assert!(self.0.is_null());
    }
}

#[test]
fn allocation_initialization_metadata_and_reset_match() {
    for name in [
        c"MD5",
        c"murmur3",
        c"RIPEMD128",
        c"RIPEMD160",
        c"RIPEMD256",
        c"RIPEMD320",
        c"SHA160",
        c"SHA224",
        c"sHa256",
        c"SHA512/224",
        c"SHA512/256",
        c"SHA384",
        c"SHA512",
        c"CRC32",
        c"adler32",
    ] {
        let mut safe = HashContext::new(name).unwrap();
        let reference = Reference::new(name);
        // SAFETY: reference is live and C returns an immutable static string.
        let expected_name = unsafe { CStr::from_ptr(ffi::av_hash_get_name(reference.0)) };
        // SAFETY: reference's algorithm is valid and the operation only reads.
        let size = unsafe { ffi::av_hash_get_size(reference.0) } as usize;
        assert_eq!(safe.as_ref().name(), expected_name);
        assert_eq!(safe.as_ref().size(), size);
        for bytes in [b"".as_slice(), b"abc", &[0; 257][..]] {
            let mut expected = vec![0; size];
            // SAFETY: both contexts are independent, initialized sole-owned
            // allocations with no outstanding handles. Input is read only for
            // its length and not retained. Each output has the digest's size.
            // Each context is finalized once and reset before its next use.
            unsafe {
                ffi::av_hash_update(reference.0, bytes.as_ptr(), bytes.len());
                ffi::av_hash_final(reference.0, expected.as_mut_ptr());
            }
            safe.as_mut().update(bytes);
            assert_eq!(safe.as_mut().finalize(), expected, "{name:?}");
            safe.as_mut().as_mut().init();
            // SAFETY: uniquely owned valid context, reset after finalization.
            unsafe { ffi::av_hash_init(reference.0) };
            assert_eq!(safe.as_mut().as_ref().name(), expected_name);
            assert_eq!(safe.as_ref().size(), size);
        }
    }
}

#[test]
fn failed_allocations_match_and_clear_the_output() {
    for name in [c"", c"unknown", c"SHA999", c"\xff"] {
        let mut context = ptr::null_mut();
        // SAFETY: writable output slot, live NUL-terminated name; C clears
        // the slot and allocates nothing for these unsupported names.
        let expected = unsafe { ffi::av_hash_alloc(&mut context, name.as_ptr()) };
        assert!(context.is_null());
        assert_eq!(HashContext::new(name).err(), Some(expected));
    }
}

use ffmpeg::libavutil::hash;

#[test]
fn batch10_registry_indices_match_c() {
    for i in [i32::MIN, -1].into_iter().chain(0..64).chain([i32::MAX]) {
        // SAFETY: C checks every integer index and returns null or a static
        // immutable NUL-terminated string which is never freed.
        let ptr = unsafe { ffi::av_hash_names(i) };
        let raw = if ptr.is_null() {
            None
        } else {
            // SAFETY: non-null hashdesc names are immutable NUL-terminated statics.
            Some(unsafe { std::ffi::CStr::from_ptr(ptr) })
        };
        assert_eq!(hash::name(i), raw);
    }
}

#[test]
fn allocation_failures_match_c() {
    // The av_max_alloc limit is process-global: run in an isolated child.
    const ISOLATED: &str = "CRUSTIFY_HASH_ALLOC_LIMIT_CHILD";
    if std::env::var_os(ISOLATED).is_none() {
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "hash::allocation_failures_match_c",
                "--test-threads=1",
            ])
            .env(ISOLATED, "1")
            .status()
            .unwrap();
        assert!(status.success());
        return;
    }
    let names = [
        c"MD5",
        c"murmur3",
        c"RIPEMD320",
        c"SHA256",
        c"SHA512",
        c"CRC32",
        c"adler32",
    ];
    // A limit at which the parent context fits (adler32, which has no child,
    // succeeds) while another algorithm fails reaches av_hash_alloc's
    // partial-failure path: the child allocation fails and C frees the parent.
    let mut child_failures = 0;
    for limit in [0, 1, 8, 16, 24, 32, 48, 64, 96, 128, 192, 256, 512, 4096] {
        ffmpeg::libavutil::mem::set_max_alloc(limit);
        let mut failures = 0;
        let mut parent_fits = false;
        for name in names {
            let mut context = ptr::null_mut();
            // SAFETY: valid output slot and NUL-terminated name; on success C
            // transfers an allocation this test frees below.
            let expected = unsafe { ffi::av_hash_alloc(&mut context, name.as_ptr()) };
            let actual = HashContext::new(name);
            if expected < 0 {
                assert!(context.is_null());
                assert_eq!(actual.err(), Some(expected), "{name:?} at {limit}");
                failures += 1;
            } else {
                assert!(!context.is_null());
                assert_eq!(actual.unwrap().as_ref().name(), name_of(context));
                parent_fits |= name == c"adler32";
                // SAFETY: this test owns the fresh context and frees it once.
                unsafe { ffi::av_hash_freep(&mut context) };
            }
        }
        if parent_fits {
            child_failures += failures;
        }
    }
    ffmpeg::libavutil::mem::set_max_alloc(i32::MAX as usize);
    assert!(
        child_failures > 0,
        "no limit separated parent and child sizes"
    );
}

fn name_of(context: *const ffi::AVHashContext) -> &'static CStr {
    // SAFETY: context is a live allocated context with a valid algorithm; C
    // returns a NUL-terminated name in static storage.
    unsafe { CStr::from_ptr(ffi::av_hash_get_name(context)) }
}

#[test]
fn all_algorithms_output_formats_and_capacities() {
    for index in 0..15 {
        let name = hash::name(index).unwrap();
        let mut safe = HashContext::new(name).unwrap();
        let raw = Reference::new(name);
        let data: Vec<_> = (0..260).map(|i| (i * 31) as u8).collect();
        for format in 0..4 {
            for capacity in [0, 1, 2, 3, 4, 7, 16, 31, 32, 63, 64, 65, 89, 129, 140] {
                if (format == 2 && capacity < 2) || (format == 3 && capacity == 0) {
                    continue;
                }
                safe.as_mut().init();
                // SAFETY: fixture owns an allocated context with a valid algorithm.
                unsafe { ffi::av_hash_init(raw.0) };
                for chunk in data[1..].chunks(17) {
                    safe.as_mut().update(chunk);
                    // SAFETY: valid initialized context and readable chunk; no retention.
                    unsafe { ffi::av_hash_update(raw.0, chunk.as_ptr(), chunk.len()) };
                }
                let size = if format == 0 {
                    safe.as_ref().size()
                } else {
                    capacity
                };
                let mut actual = vec![0xA5; size];
                let mut expected = actual.clone();
                let mut empty = 0;
                let ptr = if size == 0 {
                    &raw mut empty
                } else {
                    expected.as_mut_ptr()
                };
                // SAFETY: independently initialized context, output is digest-sized
                // for final, int-sized for bin/hex/b64, and invalid text capacities
                // are excluded. Empty binary output uses a live dummy byte.
                unsafe {
                    match format {
                        0 => ffi::av_hash_final(raw.0, ptr),
                        1 => ffi::av_hash_final_bin(raw.0, ptr, size as i32),
                        2 => ffi::av_hash_final_hex(raw.0, ptr, size as i32),
                        _ => ffi::av_hash_final_b64(raw.0, ptr, size as i32),
                    }
                }
                match format {
                    0 => actual = safe.as_mut().finalize(),
                    1 => safe.as_mut().finalize_bin(&mut actual),
                    2 => safe.as_mut().finalize_hex(&mut actual).unwrap(),
                    _ => safe.as_mut().finalize_b64(&mut actual).unwrap(),
                }
                assert_eq!(actual, expected, "{name:?} format={format} capacity={size}");
            }
        }
    }
}

/// Outputs longer than C's `int` size parameter, which C callers offer as
/// `INT_MAX` bytes (crypto wave-1 review,
/// `equiv/hash-and-hmac-reject-outputs-beyond-c-ranges`).
mod beyond_int_max {
    use super::Reference;
    use ffmpeg::libavutil::{ffi, hash::HashContext};

    const LEN: usize = i32::MAX as usize + 1;
    const HEAD: usize = 160;

    /// BSan cost grows with a 2 GiB allocation's history (see the campaign's
    /// test recipes); the plain and other sanitized runs cover these.
    fn under_bsan() -> bool {
        std::env::var("CRUSTIFY_FFMPEG_VARIANT").as_deref() == Ok("bsan")
    }

    /// A zeroed allocation, so only the pages written are ever mapped, with
    /// a recognisable head and last byte.
    fn buffer() -> Vec<u8> {
        let mut buffer = vec![0u8; LEN];
        buffer[..HEAD].fill(0xAA);
        buffer[LEN - 1] = 0xAA;
        buffer
    }

    #[test]
    fn text_and_binary_finals_into_outputs_beyond_int_max_follow_c() {
        if under_bsan() {
            return;
        }
        for name in [c"SHA256", c"CRC32", c"SHA512"] {
            for format in 0..3 {
                let raw = Reference::new(name);
                let mut expected = buffer();
                // SAFETY: initialized context, readable input and an output
                // writable for `INT_MAX` bytes; C writes at most that many.
                unsafe {
                    ffi::av_hash_update(raw.0, b"crustify".as_ptr(), 8);
                    match format {
                        0 => ffi::av_hash_final_bin(raw.0, expected.as_mut_ptr(), i32::MAX),
                        1 => ffi::av_hash_final_hex(raw.0, expected.as_mut_ptr(), i32::MAX),
                        _ => ffi::av_hash_final_b64(raw.0, expected.as_mut_ptr(), i32::MAX),
                    }
                }
                let (head, last) = (expected[..HEAD].to_vec(), expected[LEN - 1]);
                drop(expected);
                let mut safe = HashContext::new(name).unwrap();
                safe.as_mut().update(b"crustify");
                let mut actual = buffer();
                match format {
                    0 => safe.as_mut().finalize_bin(&mut actual),
                    1 => safe.as_mut().finalize_hex(&mut actual).unwrap(),
                    _ => safe.as_mut().finalize_b64(&mut actual).unwrap(),
                }
                assert_eq!(actual[..HEAD], head, "{name:?} format={format}");
                // C cannot address the last byte: the text formats leave it,
                // and the binary format's zero padding reaches it.
                let padded = if format == 0 { 0 } else { last };
                assert_eq!(actual[LEN - 1], padded, "{name:?} format={format}");
            }
        }
    }
}
