//! Owned and borrowed views of the opaque Twofish key schedule.
//!
//! C stores inline subkeys and lookup tables, never the supplied key pointer.
//! The public header exposes no fields or clone routine. Storage remains
//! C-allocated and is released by the [`TwofishFree`] policy.

use core::{fmt, ptr::NonNull};
use ffibox::{CBox, CDrop, define_ctype};

use super::{ffi, mem::Heap};

define_ctype!(
    /// Wraps: AVTWOFISH
    ///
    /// An initialized Twofish context. Its private C fields are not exposed.
    /// No `Send`/`Sync` guarantee is added.
    ///
    /// An exclusive reborrow freezes its parent:
    /// ```compile_fail
    /// use ffmpeg::libavutil::twofish::Twofish;
    /// let (mut owner, _) = Twofish::new(&[0; 16], 128).unwrap();
    /// let mut parent = owner.as_mut();
    /// let mut child = parent.as_mut();
    /// parent.init(&[1; 16], 128).unwrap();
    /// child.init(&[2; 16], 128).unwrap();
    /// ```
    /// A shared handle keeps its owner alive:
    /// ```compile_fail
    /// use ffmpeg::libavutil::twofish::Twofish;
    /// let (owner, _) = Twofish::new(&[0; 16], 128).unwrap();
    /// let shared = owner.as_ref();
    /// drop(owner);
    /// let _copy = shared;
    /// ```
    Twofish, TwofishRef, TwofishMut, ffi::AVTWOFISH
);

/// Sole ownership of a C-allocated Twofish context, freed with `av_free`.
///
/// The opaque layout has no Rust size, so there is no Rust-sized allocation:
///
/// ```compile_fail,E0277
/// use ffmpeg::libavutil::twofish::TwofishBox;
/// let _ = TwofishBox::new_uninit();
/// ```
pub type TwofishBox = CBox<Twofish, TwofishFree>;

/// Storage-only release of an `av_twofish_alloc` context. Unlike [`Heap`], it
/// offers no generic allocator: the opaque layout's Rust size is zero, so
/// only C can size a Twofish context.
#[derive(Clone, Copy, Debug, Default)]
pub struct TwofishFree;

/// Wraps: av_free
// SAFETY: TwofishBox adopts only `av_twofish_alloc` storage, which owns no sub-allocations;
// Heap's av_free releases that av_malloc-family block once without reading it.
unsafe impl CDrop<Twofish> for TwofishFree {
    unsafe fn c_drop(&self, ptr: NonNull<Twofish>) {
        // SAFETY: CDrop's caller transfers the unique av_malloc-family claim.
        unsafe { <Heap as CDrop<Twofish>>::c_drop(&Heap, ptr) }
    }
}

/// C's successful initialization return value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitStatus {
    /// The bit count was exactly 128, 192 or 256 (C returns zero).
    Exact,
    /// C padded or truncated the supplied key (C returns one).
    Adjusted,
}

/// Initialization failed before modifying the context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitError {
    /// Negative bit counts are rejected by C with `AVERROR(EINVAL)`.
    NegativeKeyBits,
    /// C would read more bytes than the supplied key contains.
    KeyTooShort { required: usize, actual: usize },
    /// The C allocator returned null.
    AllocationFailed,
}

impl fmt::Display for InitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NegativeKeyBits => f.write_str("negative Twofish key bit count"),
            Self::KeyTooShort { required, actual } => {
                write!(f, "Twofish requires {required} key bytes, got {actual}")
            }
            Self::AllocationFailed => f.write_str("Twofish allocation failed"),
        }
    }
}
impl std::error::Error for InitError {}

fn validate_key(key: &[u8], key_bits: i32) -> Result<(), InitError> {
    if key_bits < 0 {
        return Err(InitError::NegativeKeyBits);
    }
    // C reads floor(bits / 8), capped at 32; partial bytes are ignored.
    let required = (key_bits.min(256) / 8) as usize;
    if key.len() < required {
        return Err(InitError::KeyTooShort {
            required,
            actual: key.len(),
        });
    }
    Ok(())
}

impl Twofish {
    /// Wraps: av_twofish_alloc
    ///
    /// Allocate a fully initialized context and return C's initialization
    /// status. Zero bits (including an empty key), partial bytes and counts
    /// above 256 are accepted as in C: missing bits are zero-padded, partial
    /// bytes ignored, and keys above 256 bits truncated to 32 bytes.
    pub fn new(key: &[u8], key_bits: i32) -> Result<(TwofishBox, InitStatus), InitError> {
        validate_key(key, key_bits)?;
        // SAFETY: fresh zero-filled C storage has only inline scalar fields,
        // and TwofishFree releases it using the paired av_free. No owner escapes
        // before init fills all fields subsequently read by the C cipher.
        let mut owner = unsafe { TwofishBox::from_c(ffi::av_twofish_alloc()) }
            .ok_or(InitError::AllocationFailed)?;
        let status = owner.as_mut().init(key, key_bits)?;
        Ok((owner, status))
    }
}

impl TwofishMut<'_> {
    /// Wraps: av_twofish_init
    ///
    /// Replace the key schedule, preserving C's padding/truncation status.
    /// An invalid bit count or short slice leaves the old schedule unchanged.
    /// The key is read only during this call and need not outlive the context.
    pub fn init(&mut self, key: &[u8], key_bits: i32) -> Result<InitStatus, InitError> {
        validate_key(key, key_bits)?;
        // SAFETY: this handle exclusively borrows a full C context; validation
        // guarantees nonnegative bits and at least min(bits, 256)/8 readable
        // bytes. Even for zero bytes the slice pointer is nonnull. C copies
        // the key into local storage, writes only the context, and retains no
        // pointers. It returns 0 or 1 after filling the entire key schedule.
        let status = unsafe { ffi::av_twofish_init(self.as_mut_ptr(), key.as_ptr(), key_bits) };
        Ok(if status == 0 {
            InitStatus::Exact
        } else {
            InitStatus::Adjusted
        })
    }
}

/// Wraps: av_twofish_size
///
/// The C context's size in bytes, borrowed from immutable C static storage.
pub fn context_size() -> &'static i32 {
    // SAFETY: C defines this global as `const int = sizeof(context)`; it has
    // static lifetime and no writer anywhere in the library.
    unsafe { &ffi::av_twofish_size }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn layout_matches_ffi() {
        assert_eq!(
            core::mem::size_of::<Twofish>(),
            core::mem::size_of::<ffi::AVTWOFISH>()
        );
        assert_eq!(
            core::mem::align_of::<Twofish>(),
            core::mem::align_of::<ffi::AVTWOFISH>()
        );
        assert_eq!(
            core::mem::size_of::<TwofishRef<'_>>(),
            core::mem::size_of::<usize>()
        );
        assert_eq!(
            core::mem::size_of::<TwofishMut<'_>>(),
            core::mem::size_of::<usize>()
        );
    }

    #[test]
    fn validation_and_errors() {
        assert_eq!(validate_key(&[], i32::MIN), Err(InitError::NegativeKeyBits));
        assert_eq!(validate_key(&[], 7), Ok(()));
        assert_eq!(
            validate_key(&[], 8),
            Err(InitError::KeyTooShort {
                required: 1,
                actual: 0
            })
        );
        assert_eq!(validate_key(&[0; 32], i32::MAX), Ok(()));
        assert_eq!(
            InitError::NegativeKeyBits.to_string(),
            "negative Twofish key bit count"
        );
        assert_eq!(
            InitError::KeyTooShort {
                required: 32,
                actual: 1
            }
            .to_string(),
            "Twofish requires 32 key bytes, got 1"
        );
        assert_eq!(
            InitError::AllocationFailed.to_string(),
            "Twofish allocation failed"
        );
        assert_eq!(InitStatus::Exact, InitStatus::Exact);
        assert_ne!(InitStatus::Exact, InitStatus::Adjusted);
    }

    #[test]
    #[cfg(not(miri))]
    fn raw_ownership_round_trip() {
        let (owner, _) = Twofish::new(&[7; 16], 128).unwrap();
        let ptr = owner.into_c();
        // SAFETY: into_c transferred the sole initialized allocation and no
        // borrow remains. TwofishFree is still the matching release policy.
        let owner = unsafe { TwofishBox::from_c(ptr) }.unwrap();
        drop(owner);
    }
}
