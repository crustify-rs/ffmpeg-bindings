//! Safe wrappers over `libavutil/hmac.h`.
//!
//! [`HmacBox`] owns both the opaque context and its private MD5, SHA or
//! SHA512 hash allocation. C copies/hashes the key during initialization;
//! it does not retain the input pointer. There is no C clone operation, and
//! byte copying the opaque context would duplicate its owned hash pointer.

use ffibox::{CBox, CPlainElem, define_ctype, impl_cdrop};

use super::ffi;

/// Wraps: AVHMACType
/// Handwritten
///
/// The hash algorithm selected by an HMAC context.
///
/// This resource-free value preserves every C integer encoding, including
/// unknown values (which `av_hmac_alloc` rejects with NULL). No ffibox
/// ownership primitive is needed for an enum.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct HmacType(ffi::AVHMACType);

impl HmacType {
    pub const MD5: Self = Self(ffi::AVHMACType::AV_HMAC_MD5);
    pub const SHA1: Self = Self(ffi::AVHMACType::AV_HMAC_SHA1);
    pub const SHA224: Self = Self(ffi::AVHMACType::AV_HMAC_SHA224);
    pub const SHA256: Self = Self(ffi::AVHMACType::AV_HMAC_SHA256);
    pub const SHA384: Self = Self(ffi::AVHMACType::AV_HMAC_SHA384);
    pub const SHA512: Self = Self(ffi::AVHMACType::AV_HMAC_SHA512);

    /// Preserves a C algorithm value, including an unknown encoding.
    pub const fn from_raw(raw: ffi::AVHMACType) -> Self {
        Self(raw)
    }

    /// The C encoding of this algorithm.
    pub const fn into_raw(self) -> ffi::AVHMACType {
        self.0
    }
}

impl From<ffi::AVHMACType> for HmacType {
    fn from(raw: ffi::AVHMACType) -> Self {
        Self::from_raw(raw)
    }
}

impl From<HmacType> for ffi::AVHMACType {
    fn from(value: HmacType) -> Self {
        value.into_raw()
    }
}

// SAFETY: the transparent wrapper and bindgen's integer newtype have the
// same layout; every integer bit pattern is valid and has no C-side identity.
unsafe impl CPlainElem for HmacType {}

// SAFETY: the generated handles name the opaque ffi::AVHMAC pointer type.
// C alone allocates/accesses its body; no Rust inline value is constructed.
define_ctype!(
    /// Wraps: AVHMAC
    ///
    /// Opaque layout reached only through C. A safe constructor initializes
    /// both the algorithm callbacks and the hash state with a key.
    /// The body is private to `hmac.c`, so there are no field accessors,
    /// zeroed values, inline constructors, or byte-copy clone strategy.
    Hmac,
    HmacRef,
    HmacMut,
    ffi::AVHMAC
);

/// Wraps: av_hmac_free
///
/// Releases the private hash allocation, then the HMAC context.
#[derive(Clone, Copy, Debug, Default)]
pub struct HmacFree;

impl_cdrop!(HmacFree, Hmac, ffi::av_hmac_free);

/// Sole ownership of a keyed HMAC context. Handles borrow the owner, and
/// neither a handle nor its reborrow can outlive it.
///
/// ```compile_fail,E0597
/// use ffmpeg::libavutil::hmac::{Hmac, HmacType};
/// let view;
/// {
///     let owner = Hmac::new(HmacType::MD5, b"key").unwrap();
///     view = owner.as_ref();
/// }
/// std::hint::black_box(view);
/// ```
pub type HmacBox = CBox<Hmac, HmacFree>;

impl Hmac {
    /// Wraps: av_hmac_alloc
    ///
    /// Allocates and keys a context. Returns -12 on allocation failure (or
    /// an unknown algorithm value), and -22 if the key length exceeds C's
    /// `unsigned int` range. Even an empty key initializes the hash state.
    ///
    /// The algorithm is a typed value; unknown integer encodings preserved
    /// by [`HmacType`] are rejected by C without forming an owner.
    pub fn new(algorithm: HmacType, key: &[u8]) -> Result<HmacBox, i32> {
        key_len(key.len())?;
        // SAFETY: C checks the algorithm value and returns either NULL or a
        // fresh context with valid callbacks and a private owned hash object.
        let raw = unsafe { ffi::av_hmac_alloc(algorithm.into_raw()) };
        // SAFETY: a non-NULL result is uniquely owned, released together with
        // its hash by av_hmac_free. That destructor also accepts the state
        // before key initialization, so early failure cannot leak it.
        let mut context = unsafe { HmacBox::from_c(raw) }.ok_or(-12)?;
        context.as_mut().init(key)?;
        Ok(context)
    }
}

fn key_len(len: usize) -> Result<u32, i32> {
    u32::try_from(len).map_err(|_| -22)
}

impl HmacMut<'_> {
    /// Wraps: av_hmac_init
    ///
    /// Restarts the hash with a copied authentication key. Long keys are
    /// hashed down to the selected digest size by C. Returns -22 before
    /// touching the context if the key length exceeds `unsigned int`.
    pub fn init(&mut self, key: &[u8]) -> Result<(), i32> {
        let len = key_len(key.len())?;
        // Give memcpy a live pointer even for an empty key, not the dangling
        // sentinel of an empty Rust slice. C keeps neither pointer.
        let empty = 0u8;
        let source = if key.is_empty() {
            &raw const empty
        } else {
            key.as_ptr()
        };
        // SAFETY: the handle exclusively borrows a context from av_hmac_alloc
        // with a valid algorithm/hash allocation. source supplies len readable
        // bytes; C copies or hashes them and retains only its own key bytes.
        unsafe { ffi::av_hmac_init(self.as_mut_ptr(), source, len) };
        Ok(())
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn representations_and_conversions() {
        assert_eq!(size_of::<HmacType>(), size_of::<ffi::AVHMACType>());
        assert_eq!(align_of::<HmacType>(), align_of::<ffi::AVHMACType>());
        for raw in [0, 1, 2, 3, 4, 5, 6, u32::MAX, 1 << 31] {
            let c = ffi::AVHMACType(raw);
            let value = HmacType::from(c);
            assert_eq!(ffi::AVHMACType::from(value), c);
            let copy = value;
            assert_eq!(value, copy);
            assert!(!format!("{value:?}").is_empty());
        }
    }

    #[test]
    fn length_conversion_rejects_truncation() {
        assert_eq!(key_len(0), Ok(0));
        assert_eq!(key_len(u32::MAX as usize), Ok(u32::MAX));
        if usize::BITS > u32::BITS {
            assert_eq!(key_len(u32::MAX as usize + 1), Err(-22));
        }
    }

    #[test]
    fn rejects_unknown_algorithm_and_raw_adoption_roundtrips() {
        assert!(matches!(
            Hmac::new(HmacType::from_raw(ffi::AVHMACType(u32::MAX)), b"key"),
            Err(-12)
        ));
        let owner = Hmac::new(HmacType::SHA512, b"key").unwrap();
        let raw = owner.into_c();
        // SAFETY: into_c surrendered this keyed context and its private hash;
        // no other owner or handle exists, and HmacFree is its paired releaser.
        let mut restored = unsafe { HmacBox::from_c(raw) }.unwrap();
        restored.as_mut().init(b"new key").unwrap();
    }
}
