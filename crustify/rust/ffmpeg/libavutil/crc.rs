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

/// The two array sizes accepted by C's CRC table initializer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TableSize {
    Compact,
    Fast,
}

/// A private, initialized CRC table. Entries cannot be changed by callers:
/// C may put an architecture-specific dispatch tag and constants in them.
/// All storage is Rust-owned; C only borrows it during synchronous calls.
#[derive(Clone, Debug)]
pub struct Table {
    entries: Box<[u32]>,
}

impl Table {
    /// Wraps: av_crc_init
    ///
    /// Initialize a CRC table for a polynomial of 8 through 32 bits. For
    /// little-endian bit order, the polynomial is in reversed representation.
    /// Return C's negative error for invalid bits or polynomial.
    pub fn new(
        little_endian: bool,
        bits: u8,
        polynomial: u32,
        size: TableSize,
    ) -> Result<Self, i32> {
        let len = match size {
            TableSize::Compact => 257,
            TableSize::Fast => 1024,
        };
        // Zero initialization also covers unused entries on accelerated paths.
        let mut entries = vec![0u32; len].into_boxed_slice();
        // SAFETY: entries is exclusive, aligned u32 storage of exactly one of
        // the two accepted byte sizes. C checks bits/poly before shifts and
        // only writes this allocation. It neither frees nor retains it.
        let ret = unsafe {
            ffi::av_crc_init(
                entries.as_mut_ptr(),
                i32::from(little_endian),
                i32::from(bits),
                polynomial,
                (len * 4) as i32,
            )
        };
        if ret < 0 {
            Err(ret)
        } else {
            Ok(Self { entries })
        }
    }

    /// Read-only table words, including initialized but unused entries.
    /// Their representation is architecture-dependent.
    pub fn entries(&self) -> &[u32] {
        &self.entries
    }

    /// Wraps: av_crc
    ///
    /// Update a CRC from its previous value (or the polynomial's initial value).
    /// For non-little-endian tables C uses its documented byte-swapped form.
    pub fn update(&self, initial: u32, input: &[u8]) -> u32 {
        // Give even a short/empty input a live prefix and suffix: the generic
        // fast C path forms `end - 3` before checking the loop condition. This
        // keeps that pointer within an allocation, including at length zero.
        if input.len() < 3 {
            let mut padded = [0u8; 8];
            padded[3..3 + input.len()].copy_from_slice(input);
            // SAFETY: the private table came from successful av_crc_init.
            // Input lives at offset 3 in an 8-byte allocation, so C's pointer
            // arithmetic is in bounds, and it reads only input.len() bytes.
            unsafe {
                ffi::av_crc(
                    self.entries.as_ptr(),
                    initial,
                    padded.as_ptr().add(3),
                    input.len(),
                )
            }
        } else {
            // SAFETY: the table is initialized and unmodified; the input is
            // readable for its length, and `end - 3` remains in bounds.
            // C retains no pointers and writes neither shared input.
            unsafe { ffi::av_crc(self.entries.as_ptr(), initial, input.as_ptr(), input.len()) }
        }
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

    #[test]
    fn invalid_parameters_are_errors() {
        for bits in [0, 7, 33, 255] {
            assert!(Table::new(false, bits, 1, TableSize::Compact).is_err());
        }
        assert!(Table::new(true, 8, 256, TableSize::Fast).is_err());
    }

    #[test]
    fn table_traits_and_debug() {
        let size = TableSize::Fast;
        assert_eq!(size, TableSize::Fast);
        assert_ne!(size, TableSize::Compact);
        assert_eq!(format!("{size:?}"), "Fast");
        let table = Table::new(true, 32, 0xedb88320, size).unwrap();
        let clone = table.clone();
        assert_eq!(table.entries(), clone.entries());
        assert!(format!("{clone:?}").starts_with("Table { entries:"));
    }
}
