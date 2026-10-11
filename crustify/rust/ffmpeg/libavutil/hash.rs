//! Owned and borrowed contexts for `libavutil/hash.h`.
//!
//! C keeps the representation private. A context owns its selected algorithm's
//! storage; CRC32 instead borrows an immutable static table, and adler32 needs
//! no child allocation. C has no clone, refcount, or field-disposal primitive.

use core::ffi::{CStr, c_int};
use core::ptr::{self, NonNull};

use ffibox::{CBox, CDrop, define_ctype};

use super::ffi;

define_ctype!(
    /// Wraps: AVHashContext
    ///
    /// A C hash context, owned as a [`HashContextBox`]. The public C API is
    /// opaque, so there are no field accessors or inline Rust-owned values.
    /// Handles require an allocated context with a valid algorithm; shared
    /// operations only read metadata and do not inspect the hashing state.
    HashContext,
    HashContextRef,
    HashContextMut,
    ffi::AVHashContext
);

/// Wraps: av_hash_freep
///
/// Releases the context and its algorithm storage exactly once. The CRC
/// table is static and is not freed.
#[derive(Clone, Copy, Debug, Default)]
pub struct HashContextFree;

// SAFETY: av_hash_freep releases precisely one av_hash_alloc allocation and
// its children, matching every successful constructor branch.
unsafe impl CDrop<HashContext> for HashContextFree {
    unsafe fn c_drop(&self, ptr: NonNull<HashContext>) {
        // Adapts the C pointer-to-pointer release to the sole-owner policy.
        let mut slot = ptr.as_ptr().cast::<ffi::AVHashContext>();
        // SAFETY: CDrop's caller transfers the uniquely owned av_hash_alloc
        // allocation. C frees its child and then the context, and clears only
        // this writable local slot; it does not retain the slot's address.
        unsafe { ffi::av_hash_freep(&mut slot) };
    }
}

/// The sole owner of a C hash context. Not `Clone`: copying its private bytes
/// would duplicate child ownership. Thread traits are intentionally withheld.
///
/// A handle cannot outlive its owner:
/// ```compile_fail
/// use ffmpeg::libavutil::hash::HashContext;
/// let view = {
///     let hash = HashContext::new(c"MD5").unwrap();
///     hash.as_ref()
/// };
/// let _ = view.size();
/// ```
/// Shared and exclusive handles cannot alias:
/// ```compile_fail
/// use ffmpeg::libavutil::hash::HashContext;
/// let mut hash = HashContext::new(c"MD5").unwrap();
/// let shared = hash.as_ref();
/// hash.as_mut().init();
/// let _ = shared.size();
/// ```
/// An exclusive reborrow freezes its parent handle:
/// ```compile_fail
/// use ffmpeg::libavutil::hash::HashContext;
/// let mut hash = HashContext::new(c"MD5").unwrap();
/// let mut parent = hash.as_mut();
/// let mut child = parent.as_mut();
/// parent.init();
/// child.init();
/// ```
pub type HashContextBox = CBox<HashContext, HashContextFree>;

impl HashContext {
    /// Wraps: av_hash_alloc
    ///
    /// Allocates and initializes the named algorithm (case-insensitive).
    /// Returns C's negative error code for an unknown algorithm or allocation
    /// failure. The name is read only during allocation, never retained.
    #[allow(clippy::new_ret_no_self)]
    pub fn new(name: &CStr) -> Result<HashContextBox, c_int> {
        let mut context = ptr::null_mut();
        // SAFETY: context is a writable local output slot; name is a live
        // NUL-terminated string. C reads the name, retains no input pointer,
        // and hands over a fresh allocation on success, NULL on failure.
        let result = unsafe { ffi::av_hash_alloc(&mut context, name.as_ptr()) };
        if result < 0 {
            return Err(result);
        }
        // SAFETY: successful av_hash_alloc produced a uniquely owned context
        // with a valid algorithm and children, releasable by av_hash_freep.
        // The policy may release it even before hashing state is initialized.
        let mut owner = unsafe { HashContextBox::from_c(context) }
            .expect("av_hash_alloc succeeded without an allocation");
        owner.as_mut().init();
        Ok(owner)
    }
}

impl HashContextRef<'_> {
    /// Wraps: av_hash_get_name
    ///
    /// The algorithm's canonical name, backed by C's immutable static table.
    pub fn name(&self) -> &'static CStr {
        // SAFETY: this handle keeps a valid allocated context alive. C only
        // reads its algorithm and returns a non-NULL NUL-terminated string
        // in static const hashdesc storage, independent of the context's life.
        unsafe { CStr::from_ptr(ffi::av_hash_get_name(self.as_ptr())) }
    }

    /// Wraps: av_hash_get_size
    ///
    /// The number of bytes in this algorithm's binary digest.
    pub fn size(&self) -> usize {
        // SAFETY: the live allocated context has an algorithm selected by
        // av_hash_alloc. C only reads it and returns its positive digest size.
        unsafe { ffi::av_hash_get_size(self.as_ptr()) as usize }
    }
}

impl HashContextMut<'_> {
    /// Wraps: av_hash_init
    ///
    /// Initializes or resets the hashing state, including after finalization.
    pub fn init(&mut self) {
        // SAFETY: this exclusive handle keeps an allocated context and its
        // children alive, excludes competing access, and C initializes all
        // hashing state for its valid algorithm. No pointer is retained.
        unsafe { ffi::av_hash_init(self.as_mut_ptr()) };
    }
}

/// The output size C is offered: a buffer longer than `INT_MAX` bytes is
/// offered as `INT_MAX` bytes, as a C caller holding it would. No digest
/// format comes near that size, so C writes the same bytes either way.
fn c_capacity(len: usize) -> i32 {
    i32::try_from(len).unwrap_or(i32::MAX)
}

impl HashContextMut<'_> {
    /// Wraps: av_hash_update
    ///
    /// Append borrowed bytes. Empty input is a no-op; no pointer is retained.
    pub fn update(&mut self, data: &[u8]) {
        if data.is_empty() {
            return;
        }
        // SAFETY: exclusive initialized context and a nonempty readable slice
        // of its stated length. Each backend reads but never retains the input.
        unsafe { ffi::av_hash_update(self.as_mut_ptr(), data.as_ptr(), data.len()) };
    }

    /// Wraps: av_hash_final
    ///
    /// Finalize into a digest-sized owned buffer. Reset with [`Self::init`]
    /// before a new message. The current C backends remain memory-safe after
    /// finalization, but subsequent updates/finals need not hash a new message.
    pub fn finalize(&mut self) -> Vec<u8> {
        let mut output = vec![0; self.as_ref().size()];
        // SAFETY: exclusive initialized context and the runtime digest size's
        // writable buffer. C writes exactly this size and retains no pointer.
        unsafe { ffi::av_hash_final(self.as_mut_ptr(), output.as_mut_ptr()) };
        output
    }

    /// Wraps: av_hash_final_bin
    ///
    /// Finalize with truncation or zero-padding to the output length. An
    /// empty output still finalizes the context. C pads at most `INT_MAX`
    /// bytes; padding beyond them is written here.
    pub fn finalize_bin(&mut self, output: &mut [u8]) {
        let len = c_capacity(output.len());
        let mut empty = 0u8;
        let ptr = if output.is_empty() {
            &raw mut empty
        } else {
            output.as_mut_ptr()
        };
        // SAFETY: exclusive initialized context and len writable bytes. A live
        // byte stands in for empty output so C's zero-byte memcpy receives a
        // valid pointer. C truncates/pads within len and retains neither pointer.
        unsafe { ffi::av_hash_final_bin(self.as_mut_ptr(), ptr, len) };
        // Empty unless the output is longer than C could pad.
        output[len as usize..].fill(0);
    }

    /// Wraps: av_hash_final_hex
    ///
    /// Finalize into C's NUL-terminated lowercase hex representation. The
    /// buffer must contain at least two bytes; C writes nothing at smaller
    /// sizes, contrary to its documented termination guarantee. Returns -22
    /// without modifying the context or output for smaller buffers.
    /// Truncation, including odd-sized buffers, matches C; bytes beyond the
    /// terminating NUL are left unchanged.
    pub fn finalize_hex(&mut self, output: &mut [u8]) -> Result<(), i32> {
        if output.len() < 2 {
            return Err(-22);
        }
        let len = c_capacity(output.len());
        // SAFETY: exclusive initialized context and at least len >= 2 writable
        // bytes. C's snprintf loop stays within len, writes a terminator, and
        // retains neither context nor output pointer.
        unsafe { ffi::av_hash_final_hex(self.as_mut_ptr(), output.as_mut_ptr(), len) };
        Ok(())
    }

    /// Wraps: av_hash_final_b64
    ///
    /// Finalize into NUL-terminated Base64, truncating to the output capacity.
    /// Returns -22 before any mutation for empty output, which would make C
    /// write at `dst[-1]`, so it is never passed to C.
    /// Bytes beyond the terminating NUL are left unchanged.
    pub fn finalize_b64(&mut self, output: &mut [u8]) -> Result<(), i32> {
        if output.is_empty() {
            return Err(-22);
        }
        let len = c_capacity(output.len());
        // SAFETY: exclusive initialized context and at least len >= 1 writable
        // bytes. C copies at most len bytes and, on truncation, terminates at
        // len-1, which is in bounds. No pointer is retained.
        unsafe { ffi::av_hash_final_b64(self.as_mut_ptr(), output.as_mut_ptr(), len) };
        Ok(())
    }
}

/// Wraps: av_hash_names
///
/// Name at a C hash registry index, or `None` for an invalid index.
/// The registry and its NUL-terminated names are immutable static storage.
pub fn name(index: i32) -> Option<&'static CStr> {
    // SAFETY: every integer index is checked by C; no input pointers.
    let ptr = unsafe { ffi::av_hash_names(index) };
    if ptr.is_null() {
        None
    } else {
        // SAFETY: the non-null result points to an immutable, NUL-terminated
        // name in hash.c's static hashdesc array; it is never freed.
        Some(unsafe { CStr::from_ptr(ptr) })
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn output_lengths_and_rejected_text_capacities() {
        assert_eq!(c_capacity(0), 0);
        assert_eq!(c_capacity(i32::MAX as usize), i32::MAX);
        assert_eq!(c_capacity(i32::MAX as usize + 1), i32::MAX);
        assert_eq!(c_capacity(usize::MAX), i32::MAX);
        let mut owner = HashContext::new(c"MD5").unwrap();
        owner.as_mut().update(b"abc");
        let mut output = [0xA5];
        assert_eq!(owner.as_mut().finalize_hex(&mut []), Err(-22));
        assert_eq!(owner.as_mut().finalize_hex(&mut output), Err(-22));
        assert_eq!(owner.as_mut().finalize_b64(&mut []), Err(-22));
        assert_eq!(output, [0xA5]);
        assert_eq!(owner.as_mut().finalize(), super::super::md5::sum(b"abc"));
    }

    #[test]
    fn unknown_names_are_errors() {
        for name in [c"", c"not a hash", c"SHA999", c"\xff"] {
            assert_eq!(HashContext::new(name).err(), Some(-(ffi::EINVAL as c_int)));
        }
    }

    #[test]
    fn owner_round_trip_and_null_adoption() {
        // SAFETY: NULL adopts no allocation and must return None.
        assert!(unsafe { HashContextBox::from_c(ptr::null_mut()) }.is_none());
        let owner = HashContext::new(c"MD5").unwrap();
        let raw = owner.into_c();
        // SAFETY: into_c transferred this sole live allocation; no handles
        // remain, and the same release policy adopts it exactly once.
        let mut owner = unsafe { HashContextBox::from_c(raw) }.unwrap();
        owner.as_mut().init();
        assert_eq!(owner.as_ref().name(), c"MD5");
    }
}
