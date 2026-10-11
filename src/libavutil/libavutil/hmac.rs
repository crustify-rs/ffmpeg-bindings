//! Safe algorithm values from `libavutil/hmac.h`.

use ffibox::CPlainElem;

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
}
