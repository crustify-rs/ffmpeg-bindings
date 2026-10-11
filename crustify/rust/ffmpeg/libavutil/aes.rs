//! Safe wrappers over `libavutil/aes.h`.
//!
//! C owns allocation and the private layout. Keys are copied during initialization;
//! no caller buffer is retained. A context has no sub-allocations or C clone routine.
//! Processing borrows buffers synchronously, with distinct disjoint and in-place APIs.

use core::{fmt, mem::MaybeUninit, ptr::NonNull};
use ffibox::{CAlloc, CBox, CDrop, define_ctype};

use super::{ffi, mem::Heap};

/// The key schedule to install in an AES context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Encrypt,
    Decrypt,
}

impl Direction {
    pub(crate) fn as_int(self) -> i32 {
        i32::from(self == Self::Decrypt)
    }
}

/// Buffer validation failure. Rejected calls leave context, output and IV unchanged.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CryptError {
    LengthMismatch,
    IncompleteBlock,
    CountOverflow,
}

impl fmt::Display for CryptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::LengthMismatch => "source and destination lengths differ",
            Self::IncompleteBlock => "buffer length is not a whole number of blocks",
            Self::CountOverflow => "buffer count exceeds C's int range",
        })
    }
}

impl std::error::Error for CryptError {}

pub(crate) fn buffer_count(
    dst_len: usize,
    src_len: usize,
    block: usize,
) -> Result<i32, CryptError> {
    if dst_len != src_len {
        return Err(CryptError::LengthMismatch);
    }
    if !src_len.is_multiple_of(block) {
        return Err(CryptError::IncompleteBlock);
    }
    i32::try_from(src_len / block).map_err(|_| CryptError::CountOverflow)
}

/// Failure to construct or reinitialize an AES context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitError {
    /// AES accepts exactly 16, 24 or 32 key bytes.
    InvalidKeyLength,
    AllocationFailed,
    /// The initializer returned a negative C error code.
    InitializationFailed(i32),
}

impl fmt::Display for InitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKeyLength => f.write_str("AES key must contain 16, 24 or 32 bytes"),
            Self::AllocationFailed => f.write_str("AES allocation failed"),
            Self::InitializationFailed(code) => write!(f, "AES initialization failed ({code})"),
        }
    }
}

impl std::error::Error for InitError {}

define_ctype!(
    /// Wraps: AVAES
    ///
    /// An initialized AES context. The public C type is opaque: this layout
    /// names the FFI pointee only, never its private size or fields. Construct
    /// an owner with [`Aes::new`], not an inline value or a generic allocator.
    /// An adopted context must have a valid 128-, 192- or 256-bit key schedule.
    Aes, AesRef, AesMut, ffi::AVAES
);

/// Storage allocated by `av_aes_alloc`, released by the existing [`Heap`] policy.
#[derive(Clone, Copy, Debug, Default)]
pub struct AesFree;

// SAFETY: an AES context owns no resources besides its av_malloc-family storage.
unsafe impl CDrop<Aes> for AesFree {
    /// # Safety
    /// `ptr` must be a uniquely owned, live `av_aes_alloc` allocation.
    unsafe fn c_drop(&self, ptr: NonNull<Aes>) {
        // SAFETY: the caller transfers the live allocation to be freed exactly once.
        unsafe { Heap.c_drop(ptr) }
    }
}

// SAFETY: Heap frees storage without reading it, including before initialization.
unsafe impl CDrop<MaybeUninit<Aes>> for AesFree {
    /// # Safety
    /// `ptr` must be a uniquely owned, live `av_aes_alloc` allocation.
    unsafe fn c_drop(&self, ptr: NonNull<MaybeUninit<Aes>>) {
        // SAFETY: the caller transfers the live allocation to be freed exactly once.
        unsafe { Heap.c_drop(ptr) }
    }
}

/// Wraps: av_aes_alloc
// SAFETY: C allocates the full private body with its required alignment, not
// size_of::<Aes>() (the public forward declaration has no body). AesFree releases it.
unsafe impl CAlloc<Aes> for AesFree {
    fn c_alloc(&self) -> Option<NonNull<MaybeUninit<Aes>>> {
        // SAFETY: the allocator takes no arguments and transfers fresh storage.
        NonNull::new(unsafe { ffi::av_aes_alloc() }.cast())
    }
}

/// Sole ownership of an initialized context. No C clone or thread-safety assertion.
///
/// Handles cannot outlive the owner:
/// ```compile_fail,E0505
/// use ffmpeg::libavutil::aes::{Aes, AesRef, Direction};
/// fn inspect(_: AesRef<'_>) {}
/// let aes = Aes::new(&[0; 16], Direction::Encrypt).unwrap();
/// let view = aes.as_ref();
/// drop(aes);
/// inspect(view);
/// ```
/// A shared reborrow excludes reinitialization:
/// ```compile_fail,E0502
/// use ffmpeg::libavutil::aes::{Aes, AesRef, Direction};
/// fn inspect(_: AesRef<'_>) {}
/// let mut aes = Aes::new(&[0; 16], Direction::Encrypt).unwrap();
/// let mut handle = aes.as_mut();
/// let view = handle.as_ref();
/// handle.init(&[1; 16], Direction::Decrypt).unwrap();
/// inspect(view);
/// ```
pub type AesBox = CBox<Aes, AesFree>;

fn key_bits(key: &[u8]) -> Result<i32, InitError> {
    match key.len() {
        16 => Ok(128),
        24 => Ok(192),
        32 => Ok(256),
        _ => Err(InitError::InvalidKeyLength),
    }
}

impl Aes {
    /// Allocate and initialize a context. The key may be dropped immediately.
    pub fn new(key: &[u8], direction: Direction) -> Result<AesBox, InitError> {
        let bits = key_bits(key)?;
        let mut storage = AesBox::new_uninit().ok_or(InitError::AllocationFailed)?;
        // SAFETY: storage is the full C-allocated context, exclusively held here;
        // the checked key has exactly bits/8 readable bytes. C copies it and retains
        // no caller pointer. Direction selects a supported key schedule.
        let code = unsafe {
            // C copies the key into its private schedule.
            ffi::av_aes_init(storage.as_c_ptr(), key.as_ptr(), bits, direction.as_int())
        };
        if code < 0 {
            return Err(InitError::InitializationFailed(code));
        }
        // SAFETY: successful C initialization installed the key schedule, rounds
        // and crypt function. No handle to the construction-phase storage escaped.
        Ok(unsafe { storage.assume_init() })
    }
}

impl AesMut<'_> {
    /// Wraps: av_aes_init
    ///
    /// Replace the key schedule. Invalid key lengths leave the context unchanged.
    pub fn init(&mut self, key: &[u8], direction: Direction) -> Result<(), InitError> {
        // C changes rounds/crypt even on invalid key lengths: reject before FFI.
        let bits = key_bits(key)?;
        // SAFETY: the handle grants exclusive access to the full initialized C
        // allocation; the key has exactly bits/8 readable bytes and is not retained.
        let code = unsafe {
            // C replaces the schedule without retaining the key pointer.
            ffi::av_aes_init(self.as_mut_ptr(), key.as_ptr(), bits, direction.as_int())
        };
        if code < 0 {
            Err(InitError::InitializationFailed(code))
        } else {
            Ok(())
        }
    }
}

/// Wraps: av_aes_size
///
/// The C context's size in bytes, borrowed from immutable C static storage.
pub fn context_size() -> &'static i32 {
    // SAFETY: C defines this global as `const int = sizeof(context)`; it has
    // static lifetime and no writer anywhere in the library.
    unsafe { &ffi::av_aes_size }
}

impl AesMut<'_> {
    /// Wraps: av_aes_crypt
    ///
    /// Process whole blocks with the direction selected by initialization.
    /// C ignores its redundant decrypt argument.
    /// `None` selects ECB; `Some` selects CBC and updates the IV after each block.
    /// Buffers must have equal lengths; errors leave all state unchanged.
    /// These modes provide no authentication or padding.
    ///
    /// Use the in-place variant rather than aliasing two buffer arguments:
    /// ```compile_fail,E0502
    /// use ffmpeg::libavutil::aes::{Aes, Direction};
    /// let mut aes = Aes::new(&[0; 16], Direction::Encrypt).unwrap();
    /// let mut data = [0; 16];
    /// aes.as_mut().crypt(&mut data, &data, None).unwrap();
    /// ```
    /// A CBC IV cannot alias the output:
    /// ```compile_fail,E0499
    /// use ffmpeg::libavutil::aes::{Aes, Direction};
    /// let mut aes = Aes::new(&[0; 16], Direction::Encrypt).unwrap();
    /// let mut data = [0; 16];
    /// aes.as_mut().crypt(&mut data, &[0; 16], Some(&mut data)).unwrap();
    /// ```
    pub fn crypt(
        &mut self,
        dst: &mut [u8],
        src: &[u8],
        iv: Option<&mut [u8; 16]>,
    ) -> Result<(), CryptError> {
        let count = buffer_count(dst.len(), src.len(), 16)?;
        if count == 0 {
            return Ok(());
        }
        // SAFETY: the exclusive handle addresses an initialized full context;
        // equal-length disjoint slices hold count whole blocks, and any IV is
        // a separate writable 16-byte array. C retains no buffer pointers.
        unsafe {
            ffi::av_aes_crypt(
                self.as_mut_ptr(),
                dst.as_mut_ptr(),
                src.as_ptr(),
                count,
                iv.map_or(core::ptr::null_mut(), |iv| iv.as_mut_ptr()),
                0,
            )
        };
        Ok(())
    }

    /// In-place form of [`Self::crypt`], with the same modes and validation.
    pub fn crypt_in_place(
        &mut self,
        data: &mut [u8],
        iv: Option<&mut [u8; 16]>,
    ) -> Result<(), CryptError> {
        let count = buffer_count(data.len(), data.len(), 16)?;
        if count == 0 {
            return Ok(());
        }
        let ptr = data.as_mut_ptr();
        // SAFETY: one exclusive slice provides count whole readable/writable
        // blocks. C explicitly supports src == dst, preserving ciphertext before
        // CBC decryption writes it. The initialized context and optional IV are
        // exclusively borrowed and disjoint; no pointer is retained.
        unsafe {
            ffi::av_aes_crypt(
                self.as_mut_ptr(),
                ptr,
                ptr,
                count,
                iv.map_or(core::ptr::null_mut(), |iv| iv.as_mut_ptr()),
                0,
            )
        };
        Ok(())
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    #[cfg(not(miri))]
    fn software_fallback_processing() {
        // Isolate CPU dispatch changes before the child uses any C facility.
        const CHILD: &str = "CRUSTIFY_CRYPTO_W1_B0_SOFTWARE";
        if std::env::var_os(CHILD).is_none() {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "libavutil::aes::unit_tests::software_fallback_processing",
                ])
                .env(CHILD, "1")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        // SAFETY: this fresh child runs only this exact test; no prior C state
        // exists and zero enables no unsupported CPU feature. Never restore the
        // flags, so all C objects and buffers use the same dispatch/alignment.
        unsafe { super::super::cpu::force_cpu_flags(0) };
        for key_len in [16, 24, 32] {
            let key = vec![0x57; key_len];
            for cbc in [false, true] {
                for blocks in [0, 1, 2, 8] {
                    let mut enc = Aes::new(&key, Direction::Encrypt).unwrap();
                    let mut dec = Aes::new(&key, Direction::Decrypt).unwrap();
                    let input = vec![0xa7; blocks * 16 + 3];
                    let mut output = vec![0; input.len()];
                    let (mut ie, mut id) = ([0x73; 16], [0x73; 16]);
                    enc.as_mut()
                        .crypt(
                            &mut output[1..1 + blocks * 16],
                            &input[2..2 + blocks * 16],
                            cbc.then_some(&mut ie),
                        )
                        .unwrap();
                    dec.as_mut()
                        .crypt_in_place(&mut output[1..1 + blocks * 16], cbc.then_some(&mut id))
                        .unwrap();
                    assert_eq!(output[1..1 + blocks * 16], input[2..2 + blocks * 16]);
                    assert_eq!(ie, id);
                    enc.as_mut()
                        .crypt_in_place(&mut output[1..1 + blocks * 16], cbc.then_some(&mut ie))
                        .unwrap();
                    dec.as_mut()
                        .crypt_in_place(&mut output[1..1 + blocks * 16], cbc.then_some(&mut id))
                        .unwrap();
                    assert_eq!(output[1..1 + blocks * 16], input[2..2 + blocks * 16]);
                    assert_eq!(ie, id);
                }
            }
        }
        use super::super::crc::{CrcId, Table, TableSize, get_table};
        for (id, le, bits, poly) in [
            (CrcId::ATM_8, false, 8, 0x07),
            (CrcId::EBU_8, false, 8, 0x1d),
            (CrcId::ANSI_16, false, 16, 0x8005),
            (CrcId::CCITT_16, false, 16, 0x1021),
            (CrcId::IEEE_24, false, 24, 0x864cfb),
            (CrcId::IEEE_32, false, 32, 0x04c11db7),
            (CrcId::IEEE_32_LE, true, 32, 0xedb88320),
            (CrcId::ANSI_16_LE, true, 16, 0xa001),
        ] {
            let standard = get_table(id).unwrap();
            for size in [TableSize::Compact, TableSize::Fast] {
                let generated = Table::new(le, bits, poly, size).unwrap();
                let input = [0x57; 259];
                for len in [0, 1, 2, 3, 4, 7, 16, 255, 256] {
                    assert_eq!(
                        standard.update(0, &input[1..1 + len]),
                        generated.update(0, &input[1..1 + len])
                    );
                }
            }
        }
    }

    #[test]
    fn buffer_validation() {
        assert_eq!(buffer_count(16, 16, 16), Ok(1));
        assert_eq!(buffer_count(0, 0, 16), Ok(0));
        assert_eq!(buffer_count(8, 16, 16), Err(CryptError::LengthMismatch));
        assert_eq!(buffer_count(15, 15, 16), Err(CryptError::IncompleteBlock));
        assert_eq!(
            buffer_count(i32::MAX as usize + 1, i32::MAX as usize + 1, 1),
            Err(CryptError::CountOverflow)
        );
        for error in [
            CryptError::LengthMismatch,
            CryptError::IncompleteBlock,
            CryptError::CountOverflow,
        ] {
            assert!(!error.to_string().is_empty());
        }
    }

    #[test]
    #[cfg(not(miri))]
    fn rejected_buffers_preserve_context_output_and_iv() {
        let mut owner = Aes::new(&[0; 16], Direction::Encrypt).unwrap();
        let mut output = [9; 32];
        let mut iv = [7; 16];
        assert_eq!(
            owner.as_mut().crypt(&mut output, &[0; 16], Some(&mut iv)),
            Err(CryptError::LengthMismatch)
        );
        assert_eq!(
            owner
                .as_mut()
                .crypt_in_place(&mut output[..15], Some(&mut iv)),
            Err(CryptError::IncompleteBlock)
        );
        assert_eq!(output, [9; 32]);
        assert_eq!(iv, [7; 16]);
        let mut block = [0; 16];
        owner.as_mut().crypt_in_place(&mut block, None).unwrap();
        assert_eq!(
            block,
            [
                0x66, 0xe9, 0x4b, 0xd4, 0xef, 0x8a, 0x2c, 0x3b, 0x88, 0x4c, 0xfa, 0x59, 0xca, 0x34,
                0x2b, 0x2e
            ]
        );
    }

    #[test]
    fn key_validation_and_error_messages() {
        for len in 0..=64 {
            assert_eq!(key_bits(&vec![0; len]).is_ok(), matches!(len, 16 | 24 | 32));
        }
        assert_eq!(key_bits(&[0; 16]), Ok(128));
        assert_eq!(key_bits(&[0; 24]), Ok(192));
        assert_eq!(key_bits(&[0; 32]), Ok(256));
        assert!(!InitError::InvalidKeyLength.to_string().is_empty());
        assert!(!InitError::AllocationFailed.to_string().is_empty());
        assert!(
            InitError::InitializationFailed(-22)
                .to_string()
                .contains("-22")
        );
        assert_eq!(Direction::Encrypt.as_int(), 0);
        assert_eq!(Direction::Decrypt.as_int(), 1);
    }

    #[test]
    fn invalid_construction_does_not_call_c() {
        for len in [0, 1, 15, 17, 23, 25, 31, 33] {
            assert!(matches!(
                Aes::new(&vec![0; len], Direction::Encrypt),
                Err(InitError::InvalidKeyLength)
            ));
        }
    }

    #[test]
    fn null_adoption_and_opaque_layout() {
        assert_eq!(
            core::mem::size_of::<Aes>(),
            core::mem::size_of::<ffi::AVAES>()
        );
        assert_eq!(
            core::mem::align_of::<Aes>(),
            core::mem::align_of::<ffi::AVAES>()
        );
        // SAFETY: a null pointer transfers no allocation and creates no owner.
        assert!(unsafe { AesBox::from_c(core::ptr::null_mut()) }.is_none());
    }

    #[test]
    #[cfg(not(miri))]
    fn allocation_failure_is_reported() {
        // Isolate the process-global allocation limit from every other test.
        const MARKER: &str = "CRUSTIFY_AES_ALLOC_FAILURE_CHILD";
        if std::env::var_os(MARKER).is_some() {
            super::super::mem::set_max_alloc(1);
            assert!(matches!(
                Aes::new(&[0; 16], Direction::Encrypt),
                Err(InitError::AllocationFailed)
            ));
            assert!(matches!(
                super::super::aes_ctr::AesCtr::new(&[0; 16]),
                Err(InitError::AllocationFailed)
            ));
        } else {
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "libavutil::aes::unit_tests::allocation_failure_is_reported",
                    "--exact",
                ])
                .env(MARKER, "1")
                .status()
                .unwrap();
            assert!(status.success());
        }
    }

    #[test]
    #[cfg(not(miri))]
    fn invalid_reinit_preserves_schedule() {
        let mut owner = Aes::new(&[0; 16], Direction::Encrypt).unwrap();
        let input = [0; 16];
        let mut before = [0; 16];
        let mut after = [0; 16];
        owner.as_mut().crypt(&mut before, &input, None).unwrap();
        for len in [0, 15, 17, 33] {
            assert_eq!(
                owner.as_mut().init(&vec![0; len], Direction::Decrypt),
                Err(InitError::InvalidKeyLength)
            );
            owner.as_mut().crypt(&mut after, &input, None).unwrap();
            assert_eq!(after, before);
        }
        // AES-128 known answer with an all-zero key and block.
        assert_eq!(
            before,
            [
                0x66, 0xe9, 0x4b, 0xd4, 0xef, 0x8a, 0x2c, 0x3b, 0x88, 0x4c, 0xfa, 0x59, 0xca, 0x34,
                0x2b, 0x2e
            ]
        );
    }

    #[test]
    #[cfg(not(miri))]
    fn raw_roundtrip_and_construction_phase_drop() {
        let owner = Aes::new(&[0; 16], Direction::Encrypt).unwrap();
        let ptr = owner.into_c();
        // SAFETY: into_c transferred this initialized allocation; adopt it once.
        let owner = unsafe { AesBox::from_c(ptr) }.unwrap();
        assert_eq!(owner.as_c_ptr(), ptr);
        drop(owner);
        // Exercises storage-only release without exposing an uninitialized handle.
        drop(AesBox::new_uninit().unwrap());
    }
}
