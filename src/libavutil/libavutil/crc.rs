//! Standard polynomial identifiers from `libavutil/crc.h`.

use core::fmt;

use super::ffi;

/// Wraps: AVCRCId
/// Handwritten
///
/// A standard CRC polynomial identifier, layout-compatible with C's enum.
/// It owns no resources and needs no lifecycle or borrowed handles; ffibox
/// has no enum primitive. Unknown C values are preserved, not transmuted to
/// a restricted Rust enum. `AV_CRC_MAX` is an internal sentinel, not a public
/// polynomial, and intentionally has no associated constant.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct CrcId(ffi::AVCRCId);

impl CrcId {
    /// CRC-8, polynomial 0x07.
    pub const ATM_8: Self = Self(ffi::AVCRCId::AV_CRC_8_ATM);
    /// CRC-16, polynomial 0x8005.
    pub const ANSI_16: Self = Self(ffi::AVCRCId::AV_CRC_16_ANSI);
    /// CRC-16, polynomial 0x1021.
    pub const CCITT_16: Self = Self(ffi::AVCRCId::AV_CRC_16_CCITT);
    /// CRC-32, polynomial 0x04c11db7.
    pub const IEEE_32: Self = Self(ffi::AVCRCId::AV_CRC_32_IEEE);
    /// CRC-32 with reversed bit order, polynomial 0xedb88320.
    pub const IEEE_32_LE: Self = Self(ffi::AVCRCId::AV_CRC_32_IEEE_LE);
    /// CRC-16 with reversed bit order, polynomial 0xa001.
    pub const ANSI_16_LE: Self = Self(ffi::AVCRCId::AV_CRC_16_ANSI_LE);
    /// CRC-24, polynomial 0x864cfb.
    pub const IEEE_24: Self = Self(ffi::AVCRCId::AV_CRC_24_IEEE);
    /// CRC-8, polynomial 0x1d.
    pub const EBU_8: Self = Self(ffi::AVCRCId::AV_CRC_8_EBU);

    /// Preserves any C encoding, including unsupported identifiers.
    pub const fn from_raw(raw: ffi::AVCRCId) -> Self {
        Self(raw)
    }

    /// The identifier's exact C encoding.
    pub const fn into_raw(self) -> ffi::AVCRCId {
        self.0
    }
}

impl From<ffi::AVCRCId> for CrcId {
    fn from(raw: ffi::AVCRCId) -> Self {
        Self::from_raw(raw)
    }
}

impl From<CrcId> for ffi::AVCRCId {
    fn from(id: CrcId) -> Self {
        id.into_raw()
    }
}

impl fmt::Debug for CrcId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match *self {
            Self::ATM_8 => "ATM_8",
            Self::ANSI_16 => "ANSI_16",
            Self::CCITT_16 => "CCITT_16",
            Self::IEEE_32 => "IEEE_32",
            Self::IEEE_32_LE => "IEEE_32_LE",
            Self::ANSI_16_LE => "ANSI_16_LE",
            Self::IEEE_24 => "IEEE_24",
            Self::EBU_8 => "EBU_8",
            _ => return f.debug_tuple("CrcId").field(&self.0.0).finish(),
        };
        write!(f, "CrcId::{name}")
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn layout_and_unknown_values() {
        assert_eq!(size_of::<CrcId>(), size_of::<ffi::AVCRCId>());
        assert_eq!(align_of::<CrcId>(), align_of::<ffi::AVCRCId>());
        for value in [0, 7, 8, 9, u32::MAX] {
            let id = CrcId::from(ffi::AVCRCId(value));
            let raw: ffi::AVCRCId = id.into();
            assert_eq!(raw.0, value);
        }
        assert_eq!(
            format!("{:?}", CrcId::from_raw(ffi::AVCRCId(99))),
            "CrcId(99)"
        );
    }

    #[test]
    fn constant_debug_names() {
        for (id, name) in [
            (CrcId::ATM_8, "ATM_8"),
            (CrcId::ANSI_16, "ANSI_16"),
            (CrcId::CCITT_16, "CCITT_16"),
            (CrcId::IEEE_32, "IEEE_32"),
            (CrcId::IEEE_32_LE, "IEEE_32_LE"),
            (CrcId::ANSI_16_LE, "ANSI_16_LE"),
            (CrcId::IEEE_24, "IEEE_24"),
            (CrcId::EBU_8, "EBU_8"),
        ] {
            assert_eq!(format!("{id:?}"), format!("CrcId::{name}"));
        }
    }
}
