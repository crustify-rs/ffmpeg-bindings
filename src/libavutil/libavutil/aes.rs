//! Safe wrappers over `libavutil/aes.h`.
//!
//! C owns allocation and the private layout. Keys are copied during initialization;
//! no caller buffer is retained. A context has no sub-allocations or C clone routine.
//! Processing routines are wrapped by the symbol batch, not this lifecycle module.

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

#[cfg(test)]
mod unit_tests {
    use super::*;

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
        // SAFETY: an initialized full context and disjoint 16-byte buffers; ECB
        // uses no IV. No borrowed handles or references cover the C context.
        unsafe {
            ffi::av_aes_crypt(
                owner.as_c_ptr(),
                before.as_mut_ptr(),
                input.as_ptr(),
                1,
                core::ptr::null_mut(),
                0,
            )
        };
        for len in [0, 15, 17, 33] {
            assert_eq!(
                owner.as_mut().init(&vec![0; len], Direction::Decrypt),
                Err(InitError::InvalidKeyLength)
            );
            // SAFETY: the rejected key never reached C, preserving its valid
            // schedule; buffers have one readable/writable block, and no IV.
            unsafe {
                ffi::av_aes_crypt(
                    owner.as_c_ptr(),
                    after.as_mut_ptr(),
                    input.as_ptr(),
                    1,
                    core::ptr::null_mut(),
                    0,
                )
            };
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

/// Wraps: av_aes_size
///
/// The C context's size in bytes, borrowed from immutable C static storage.
pub fn context_size() -> &'static i32 {
    // SAFETY: C defines this global as `const int = sizeof(context)`; it has
    // static lifetime and no writer anywhere in the library.
    unsafe { &ffi::av_aes_size }
}
