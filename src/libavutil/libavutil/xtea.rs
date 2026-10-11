//! Safe layout and lifecycle for `libavutil/xtea.h`.
//!
//! The public layout has sixteen key words, although C's initializers and
//! cipher routines use only the first four. Initialization preserves the
//! remaining twelve words. No pointers or resources are stored in the object.
//! Inline values need no teardown; [`XteaBox`] uses `av_xtea_alloc` and the
//! existing [`Heap`] policy's `av_free`. There is no C refcount or cloner;
//! copies of the words are independent values, and heap clones use `av_memdup`.
//! Cipher operations are left to the separately scheduled symbol wrappers.

use core::marker::PhantomData;
use core::ptr::{NonNull, addr_of, addr_of_mut};

use ffibox::{CNew, CZeroable, define_ctype};

use super::ffi;
use super::mem::{Heap, HeapBox};

define_ctype!(
    /// Wraps: AVXTEA
    ///
    /// Layout-compatible XTEA key storage, held inline or through [`XteaBox`].
    /// Getters copy the words rather than referencing foreign memory.
    ///
    /// A handle cannot outlive its owner:
    /// ```compile_fail,E0597
    /// use ffmpeg::libavutil::xtea::Xtea;
    /// let view = {
    ///     let value = Xtea::from_key_words([0; 16]);
    ///     value.as_ref()
    /// };
    /// let _ = view.key();
    /// ```
    /// Shared and mutable reborrows cannot overlap:
    /// ```compile_fail,E0502
    /// use ffmpeg::libavutil::xtea::Xtea;
    /// let mut value = Xtea::from_key_words([0; 16]);
    /// let mut exclusive = value.as_mut();
    /// let shared = exclusive.as_ref();
    /// exclusive.set_key([1; 16]);
    /// let _ = shared.key();
    /// ```
    #[derive(Clone, Copy)]
    Xtea,
    XteaRef,
    XteaMut,
    ffi::AVXTEA
);

/// Sole owner of an `av_malloc`-family XTEA context; freed by `av_free`.
/// `XteaBox::new()` calls `av_xtea_alloc` and returns zero key words, or
/// `None` on allocation failure. Initialize through its exclusive handle.
pub type XteaBox = HeapBox<Xtea>;

// SAFETY: every field is a u32; all-zero is exactly av_xtea_alloc's initial
// state. Every accessor and initializer accepts it; av_free reads no fields.
unsafe impl CZeroable for Xtea {}

/// Wraps: av_xtea_alloc
// SAFETY: av_xtea_alloc returns a fresh aligned, zero-filled AVXTEA, or NULL.
// Xtea is layout-compatible and all-zero valid; Heap frees it with av_free.
unsafe impl CNew<Xtea> for Heap {
    fn c_new(&self) -> Option<NonNull<Xtea>> {
        // SAFETY: av_xtea_alloc has no arguments or preconditions and retains
        // no pointer to the returned storage.
        NonNull::new(unsafe { ffi::av_xtea_alloc() }.cast())
    }
}

impl Xtea {
    /// Build inline storage from all sixteen words without calling C.
    /// Every word is accepted; the cipher uses only words zero through three.
    #[must_use]
    pub const fn from_key_words(key: [u32; 16]) -> Self {
        Self(ffi::AVXTEA { key }, PhantomData)
    }

    /// Initialize inline storage with a 16-byte big-endian key.
    /// The unused twelve words start at zero.
    #[must_use]
    pub fn new(key: &[u8; 16]) -> Self {
        let mut value = Self::zeroed();
        value.as_mut().init(key);
        value
    }

    /// Initialize inline storage with a 16-byte little-endian key.
    /// The unused twelve words start at zero.
    #[must_use]
    pub fn new_le(key: &[u8; 16]) -> Self {
        let mut value = Self::zeroed();
        value.as_mut().init_le(key);
        value
    }
}

impl XteaRef<'_> {
    /// Field: AVXTEA.key
    ///
    /// Copy all sixteen words, including the twelve unused by the cipher.
    #[must_use]
    pub fn key(&self) -> [u32; 16] {
        // SAFETY: the shared handle keeps an initialized AVXTEA alive and
        // excludes writes. Projection forms no reference to foreign storage.
        unsafe { addr_of!((*self.as_ptr()).key).read() }
    }
}

impl XteaMut<'_> {
    /// Field: AVXTEA.key
    ///
    /// Replace all sixteen key words. No byte-order conversion is performed.
    pub fn set_key(&mut self, key: [u32; 16]) {
        // SAFETY: the exclusive handle grants sole access to the live object;
        // writing the projected array initializes every word without aliasing.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).key).write(key) }
    }

    /// Wraps: av_xtea_init
    ///
    /// Replace the first four words with the big-endian 16-byte key.
    /// The other twelve words are preserved. The key is not retained.
    pub fn init(&mut self, key: &[u8; 16]) {
        // SAFETY: the handle grants exclusive access to initialized storage;
        // key supplies the 16 readable bytes C consumes without retaining it.
        unsafe { ffi::av_xtea_init(self.as_mut_ptr(), key.as_ptr()) }
    }

    /// Wraps: av_xtea_le_init
    ///
    /// Replace the first four words with the little-endian 16-byte key.
    /// The other twelve words are preserved. The key is not retained.
    pub fn init_le(&mut self, key: &[u8; 16]) {
        // SAFETY: as for init: exclusive context, exactly 16 readable key
        // bytes, and neither pointer is retained by C.
        unsafe { ffi::av_xtea_le_init(self.as_mut_ptr(), key.as_ptr()) }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn ffi_free_layout_and_value_copy() {
        assert_eq!(size_of::<Xtea>(), size_of::<ffi::AVXTEA>());
        assert_eq!(align_of::<Xtea>(), align_of::<ffi::AVXTEA>());
        assert_eq!(size_of::<Xtea>(), 16 * size_of::<u32>());
        let mut original = Xtea::from_key_words([u32::MAX; 16]);
        #[allow(clippy::clone_on_copy)]
        let copy = original.clone();
        original.as_mut().set_key([0; 16]);
        assert_eq!(copy.as_ref().key(), [u32::MAX; 16]);
        assert_eq!(Xtea::zeroed().as_ref().key(), [0; 16]);
    }

    #[test]
    fn raw_owner_roundtrip() {
        let owner = XteaBox::new().unwrap();
        let ptr = owner.into_c();
        // SAFETY: into_c surrendered the sole live av_xtea_alloc allocation;
        // no intervening access occurred, and Heap is its av_free policy.
        let mut owner = unsafe { XteaBox::from_c(ptr) }.unwrap();
        owner.as_mut().set_key([u32::MAX; 16]);
        assert_eq!(owner.as_ref().key(), [u32::MAX; 16]);
    }

    #[test]
    fn allocation_failure_returns_none() {
        const MARKER: &str = "CRUSTIFY_XTEA_ALLOC_FAILURE_CHILD";
        if std::env::var_os(MARKER).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "libavutil::xtea::unit_tests::allocation_failure_returns_none",
                    "--test-threads=1",
                ])
                .env(MARKER, "1")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        super::super::mem::set_max_alloc(size_of::<Xtea>() - 1);
        let result = XteaBox::new();
        super::super::mem::set_max_alloc(usize::MAX);
        assert!(result.is_none());
    }
}
