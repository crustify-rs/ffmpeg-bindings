//! The safe constructor/reset and metadata agree with independently owned C
//! inputs. Raw update/final calls observe initialization, not a safe hashing
//! API (those symbols belong to a later batch).

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
            let mut actual = vec![0; size];
            let mut expected = vec![0; size];
            // SAFETY: both contexts are independent, initialized sole-owned
            // allocations with no outstanding handles. Input is read only for
            // its length and not retained. Each output has the digest's size.
            // Each context is finalized once and reset before its next use.
            unsafe {
                ffi::av_hash_update(safe.as_c_ptr(), bytes.as_ptr(), bytes.len());
                ffi::av_hash_update(reference.0, bytes.as_ptr(), bytes.len());
                ffi::av_hash_final(safe.as_c_ptr(), actual.as_mut_ptr());
                ffi::av_hash_final(reference.0, expected.as_mut_ptr());
            }
            assert_eq!(actual, expected, "{name:?}");
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
