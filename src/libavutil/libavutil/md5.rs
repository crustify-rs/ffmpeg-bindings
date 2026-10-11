//! Safe ownership and borrowing of the opaque `libavutil/md5.h` context.
//!
//! C retains the private layout and allocation. Constructors always initialize
//! the hash state; no Rust-sized allocation or byte-copy clone is supported.
//! MD5 is unsuitable for security-sensitive hashing.

use core::ptr::NonNull;
use ffibox::{CBox, CDrop, CNew, define_ctype};

use super::ffi;

define_ctype!(
    /// Wraps: AVMD5
    ///
    /// Opaque MD5 context, owned by [`Md5Box`] and accessed through handles.
    /// The private fields are not part of the public C API.
    Md5, Md5Ref, Md5Mut, ffi::AVMD5
);

/// Allocation and release policy for an initialized MD5 context.
#[derive(Clone, Copy, Debug, Default)]
pub struct Md5Free;

/// Sole owner of an initialized MD5 context. Drop releases the C allocation.
///
/// The opaque layout has no Rust size, so there is no Rust-sized allocation:
///
/// ```compile_fail,E0277
/// use ffmpeg::libavutil::md5::Md5Box;
/// let _ = Md5Box::new_uninit();
/// ```
pub type Md5Box = CBox<Md5, Md5Free>;

/// Wraps: av_free
// SAFETY: the owner hands this policy one uniquely owned av_malloc-family
// context allocation. The context has no child resources; av_free releases
// the whole allocation without reading its state.
unsafe impl CDrop<Md5> for Md5Free {
    unsafe fn c_drop(&self, ptr: NonNull<Md5>) {
        // SAFETY: the caller transfers the live context allocation to this
        // release policy exactly once. av_free accepts its erased pointer.
        unsafe { ffi::av_free(ptr.as_ptr().cast()) }
    }
}

/// Wraps: av_md5_alloc
// SAFETY: allocation uses C's actual private size, and init establishes every
// state invariant used by update/final. The paired policy releases via av_free.
unsafe impl CNew<Md5> for Md5Free {
    fn c_new(&self) -> Option<NonNull<Md5>> {
        // SAFETY: allocation has no preconditions and returns a fresh zeroed
        // context or NULL. NonNull rejects failure before initialization.
        let ptr = NonNull::new(unsafe { ffi::av_md5_alloc() })?;
        // SAFETY: ptr is a fresh, exclusively owned context of C's full size.
        unsafe { ffi::av_md5_init(ptr.as_ptr()) };
        Some(ptr.cast())
    }
}

impl Md5 {
    /// Allocate and initialize a context, or return `None` on allocation failure.
    pub fn new() -> Option<Md5Box> {
        Md5Box::new()
    }
}

impl Md5Mut<'_> {
    /// Wraps: av_md5_init
    ///
    /// Restart hashing, discarding the previous state.
    pub fn reset(&mut self) {
        // SAFETY: the exclusive handle keeps a full C context live and prevents
        // overlapping accesses. Init reads none of the old state or input data.
        unsafe { ffi::av_md5_init(self.as_mut_ptr()) }
    }

    /// Wraps: av_md5_update
    ///
    /// Append bytes. C reads them only during the call and retains no pointer.
    pub fn update(&mut self, data: &[u8]) {
        // A zero-length C update is a no-op. Avoid C's src + 0 arithmetic
        // on an empty Rust slice's potentially dangling sentinel pointer.
        if data.is_empty() {
            return;
        }
        // SAFETY: the handle grants exclusive access to initialized state;
        // data supplies len readable bytes from a nonempty allocation.
        // C neither mutates nor retains the input.
        unsafe { ffi::av_md5_update(self.as_mut_ptr(), data.as_ptr(), data.len()) }
    }

    /// Wraps: av_md5_final
    ///
    /// Output the digest, mutating the context as C does. Call [`Self::reset`]
    /// before starting a new message. Repeated finalization is memory-safe but
    /// hashes the padding added by the preceding finalization.
    pub fn finalize(&mut self) -> [u8; 16] {
        let mut digest = [0; 16];
        // SAFETY: exclusive initialized state and exactly 16 writable output
        // bytes; C retains neither pointer and writes every digest byte.
        unsafe { ffi::av_md5_final(self.as_mut_ptr(), digest.as_mut_ptr()) };
        digest
    }
}

/// Wraps: av_md5_size
///
/// The C context's size in bytes, borrowed from immutable C static storage.
pub fn context_size() -> &'static i32 {
    // SAFETY: C defines this global as `const int = sizeof(context)`; it has
    // static lifetime and no writer anywhere in the library.
    unsafe { &ffi::av_md5_size }
}

/// Wraps: av_md5_sum
///
/// Compute the 16-byte MD5 digest (not suitable for cryptographic security).
pub fn sum(input: &[u8]) -> [u8; 16] {
    let mut digest = [0; 16];
    let empty = 0u8;
    let source = if input.is_empty() {
        &raw const empty
    } else {
        input.as_ptr()
    };
    // SAFETY: C reads the input for this call only and writes exactly the
    // 16 digest bytes. A live byte represents empty input for C's `src + 0`;
    // the distinct Rust allocations cannot overlap.
    unsafe { ffi::av_md5_sum(digest.as_mut_ptr(), source, input.len()) };
    digest
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore)]
    fn allocation_failure() {
        const FLAG: &str = "CRUSTIFY_MD5_FAILURE_CHILD";
        if std::env::var_os(FLAG).is_some() {
            // SAFETY: this isolated child runs only this exact test, before any
            // FFmpeg allocation, so no concurrent C allocator reads or writes
            // the process-global limit. Rust's allocator does not use this limit.
            unsafe { ffi::av_max_alloc(0) };
            assert!(Md5::new().is_none());
            return;
        }
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "libavutil::md5::unit_tests::allocation_failure",
                "--test-threads=1",
            ])
            .env(FLAG, "1")
            .status()
            .unwrap();
        assert!(status.success());
    }

    #[test]
    fn layout_and_handle_sizes() {
        use core::mem::{align_of, size_of};
        assert_eq!(size_of::<Md5>(), size_of::<ffi::AVMD5>());
        assert_eq!(align_of::<Md5>(), align_of::<ffi::AVMD5>());
        assert_eq!(size_of::<Md5Ref<'_>>(), size_of::<usize>());
        assert_eq!(size_of::<Md5Mut<'_>>(), size_of::<usize>());
        assert_eq!(size_of::<Option<Md5Box>>(), size_of::<usize>());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn owner_round_trip_and_null_adoption() {
        let mut owner = Md5::new().unwrap();
        owner.as_mut().update(b"abc");
        let ptr = owner.into_c();
        // SAFETY: into_c transferred the sole initialized allocation; no handle
        // survives the move, and this adoption restores its paired releaser.
        let mut owner = unsafe { Md5Box::from_c(ptr) }.unwrap();
        assert_eq!(
            owner.as_mut().finalize(),
            [
                0x90, 0x01, 0x50, 0x98, 0x3c, 0xd2, 0x4f, 0xb0, 0xd6, 0x96, 0x3f, 0x7d, 0x28, 0xe1,
                0x7f, 0x72
            ]
        );
        // SAFETY: NULL is rejected and transfers no ownership.
        assert!(unsafe { Md5Box::from_c(core::ptr::null_mut()) }.is_none());
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn borrowed_raw_seams_preserve_identity() {
        let mut owner = Md5::new().unwrap();
        let shared = owner.as_ref();
        // SAFETY: the owner is live and only shared handles exist in this scope.
        let borrowed = unsafe { Md5Ref::from_ptr(shared.as_ptr().cast_mut()) }.unwrap();
        assert_eq!(borrowed.as_ptr(), shared.as_ptr());
        let mut exclusive = owner.as_mut();
        // SAFETY: the original exclusive handle is not used until this shorter
        // exclusive borrow ends. The owner keeps the initialized context live.
        let mut borrowed = unsafe { Md5Mut::from_ptr(exclusive.as_mut_ptr()) }.unwrap();
        borrowed.update(b"seam");
        borrowed.reset();
        assert_eq!(
            borrowed.finalize(),
            [
                0xd4, 0x1d, 0x8c, 0xd9, 0x8f, 0x00, 0xb2, 0x04, 0xe9, 0x80, 0x09, 0x98, 0xec, 0xf8,
                0x42, 0x7e
            ]
        );
    }
}
