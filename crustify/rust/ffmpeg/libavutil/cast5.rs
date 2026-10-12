//! Safe lifecycle for the opaque context in `libavutil/cast5.h`.
//!
//! C allocates and frees the context; its private round keys are not public
//! fields. A successful constructor always returns a key-initialized owner.

use core::{fmt, ptr::NonNull};

use ffibox::{CBox, CDrop, define_ctype};

use super::aes::{CryptError, Direction, buffer_count};
use super::ffi;

define_ctype!(
    /// Wraps: AVCAST5
    ///
    /// Opaque CAST-128 context. Only C knows its size: never allocate this
    /// layout by value or through a generic Rust-size allocator. Use
    /// [`Cast5::new`] to obtain a [`Cast5Box`], whose borrowed handles cannot
    /// outlive it. The context owns no sub-allocations or retained pointers.
    /// No C cloning or reference-counting lifecycle exists.
    ///
    /// A shared handle cannot outlive its owner:
    /// ```compile_fail
    /// use ffmpeg::libavutil::cast5::Cast5;
    /// let handle = {
    ///     let owner = Cast5::new(&[0; 5]).unwrap();
    ///     owner.as_ref()
    /// };
    /// let _copy = handle;
    /// ```
    /// Nor can an exclusive reborrow overlap a shared one:
    /// ```compile_fail
    /// use ffmpeg::libavutil::cast5::Cast5;
    /// let mut owner = Cast5::new(&[0; 5]).unwrap();
    /// let mut exclusive = owner.as_mut();
    /// let shared = exclusive.as_ref();
    /// exclusive.init(&[0; 16]).unwrap();
    /// let _copy = shared;
    /// ```
    /// And an owner cannot be dropped before its mutable handle:
    /// ```compile_fail
    /// use ffmpeg::libavutil::cast5::Cast5;
    /// let mut owner = Cast5::new(&[0; 5]).unwrap();
    /// let mut handle = owner.as_mut();
    /// drop(owner);
    /// handle.init(&[0; 16]).unwrap();
    /// ```
    Cast5,
    Cast5Ref,
    Cast5Mut,
    ffi::AVCAST5
);

/// Releases a C-allocated CAST5 context. Deliberately provides no generic
/// allocator or zero-argument constructor: the opaque layout has no usable
/// Rust size and encryption requires a key.
#[derive(Clone, Copy, Debug, Default)]
pub struct Cast5Free;

/// Wraps: av_free
// SAFETY: av_cast5_alloc returns sole-owned av_mallocz storage with no
// sub-allocations; av_free releases it without reading the context.
unsafe impl CDrop<Cast5> for Cast5Free {
    unsafe fn c_drop(&self, ptr: NonNull<Cast5>) {
        // SAFETY: CDrop's caller transfers the sole live allocation paired
        // with this policy. av_free releases it once, without reading it.
        unsafe { ffi::av_free(ptr.as_ptr().cast()) }
    }
}

/// Sole owner of a C-allocated, initialized CAST5 context.
pub type Cast5Box = CBox<Cast5, Cast5Free>;

/// Failure to construct or reinitialize a CAST5 context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitError {
    /// CAST5 keys must contain 5 through 16 bytes.
    InvalidKeyLength,
    /// C could not allocate the context.
    AllocationFailed,
    /// The C initializer returned a negative AVERROR code.
    C(i32),
}

impl fmt::Display for InitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidKeyLength => f.write_str("CAST5 keys must contain 5 through 16 bytes"),
            Self::AllocationFailed => f.write_str("could not allocate a CAST5 context"),
            Self::C(code) => write!(f, "CAST5 initialization failed: {code}"),
        }
    }
}

impl std::error::Error for InitError {}

fn key_bits(key: &[u8]) -> Result<i32, InitError> {
    if !(5..=16).contains(&key.len()) {
        return Err(InitError::InvalidKeyLength);
    }
    Ok((key.len() * 8) as i32)
}

impl Cast5 {
    /// Wraps: av_cast5_alloc
    ///
    /// Allocates and initializes a context using a 5–16 byte key. C copies
    /// the key during initialization, so the returned owner does not borrow
    /// it. Invalid lengths are rejected before allocation or FFI.
    pub fn new(key: &[u8]) -> Result<Cast5Box, InitError> {
        key_bits(key)?;
        // SAFETY: av_cast5_alloc has no preconditions and returns NULL or a
        // fresh zero-filled context from av_mallocz, paired with Cast5Free.
        // The opaque representation is valid immediately; only this local
        // owner can access it until init establishes the round keys.
        let mut owner = unsafe { Cast5Box::from_c(ffi::av_cast5_alloc()) }
            .ok_or(InitError::AllocationFailed)?;
        owner.as_mut().init(key)?;
        Ok(owner)
    }
}

impl Cast5Mut<'_> {
    /// Wraps: av_cast5_init
    ///
    /// Replaces the round keys using a 5–16 byte key. C does not retain the
    /// key. Invalid lengths leave the existing state unchanged.
    pub fn init(&mut self, key: &[u8]) -> Result<(), InitError> {
        let bits = key_bits(key)?;
        // SAFETY: this exclusive handle addresses a full C-allocated context,
        // and key contains exactly bits/8 initialized bytes. C reads those
        // bytes and writes only the context, retaining neither pointer.
        let result = unsafe { ffi::av_cast5_init(self.as_mut_ptr(), key.as_ptr(), bits) };
        if result < 0 {
            Err(InitError::C(result))
        } else {
            Ok(())
        }
    }
}

/// Wraps: av_cast5_size
///
/// The C context's size in bytes, borrowed from immutable C static storage.
pub fn context_size() -> &'static i32 {
    // SAFETY: C defines this global as `const int = sizeof(context)`; it has
    // static lifetime and no writer anywhere in the library.
    unsafe { &ffi::av_cast5_size }
}

impl Cast5Ref<'_> {
    /// Wraps: av_cast5_crypt
    ///
    /// Encrypt or decrypt whole blocks in ECB mode.
    /// Buffers must have equal lengths; errors leave all state unchanged.
    pub fn crypt(
        &self,
        dst: &mut [u8],
        src: &[u8],
        direction: Direction,
    ) -> Result<(), CryptError> {
        let count = buffer_count(dst.len(), src.len(), 8)?;
        if count == 0 {
            return Ok(());
        }
        // SAFETY: the shared handle addresses an initialized full context;
        // equal-length disjoint slices hold count whole blocks. ECB takes no
        // IV. C retains no buffer pointers.
        // C reads but never writes the schedule despite its mutable ABI pointer.
        unsafe {
            ffi::av_cast5_crypt(
                self.as_ptr().cast_mut(),
                dst.as_mut_ptr(),
                src.as_ptr(),
                count,
                direction.as_int(),
            )
        };
        Ok(())
    }

    /// In-place form of [`Self::crypt`], with the same modes and validation.
    pub fn crypt_in_place(&self, data: &mut [u8], direction: Direction) -> Result<(), CryptError> {
        let count = buffer_count(data.len(), data.len(), 8)?;
        if count == 0 {
            return Ok(());
        }
        let ptr = data.as_mut_ptr();
        // SAFETY: one exclusive slice provides count whole readable/writable
        // blocks. C reads each block before writing it, so src == dst is
        // supported. The initialized context is borrowed without writers and
        // no pointer is retained. C never writes the schedule despite its
        // mutable ABI pointer.
        unsafe {
            ffi::av_cast5_crypt(
                self.as_ptr().cast_mut(),
                ptr,
                ptr,
                count,
                direction.as_int(),
            )
        };
        Ok(())
    }
}

impl Cast5Ref<'_> {
    /// Wraps: av_cast5_crypt2
    ///
    /// Encrypt or decrypt whole blocks.
    /// `None` selects ECB; `Some` selects CBC and updates the IV after each block.
    /// Buffers must have equal lengths; errors leave all state unchanged.
    pub fn crypt_cbc(
        &self,
        dst: &mut [u8],
        src: &[u8],
        iv: Option<&mut [u8; 8]>,
        direction: Direction,
    ) -> Result<(), CryptError> {
        let count = buffer_count(dst.len(), src.len(), 8)?;
        if count == 0 {
            return Ok(());
        }
        // SAFETY: the shared handle addresses an initialized full context;
        // equal-length disjoint slices hold count whole blocks, and any IV is
        // a separate writable 8-byte array. C retains no buffer pointers.
        // C reads but never writes the schedule despite its mutable ABI pointer.
        unsafe {
            ffi::av_cast5_crypt2(
                self.as_ptr().cast_mut(),
                dst.as_mut_ptr(),
                src.as_ptr(),
                count,
                iv.map_or(core::ptr::null_mut(), |iv| iv.as_mut_ptr()),
                direction.as_int(),
            )
        };
        Ok(())
    }

    /// In-place form of [`Self::crypt_cbc`], with the same modes and validation.
    pub fn crypt_cbc_in_place(
        &self,
        data: &mut [u8],
        iv: Option<&mut [u8; 8]>,
        direction: Direction,
    ) -> Result<(), CryptError> {
        let count = buffer_count(data.len(), data.len(), 8)?;
        if count == 0 {
            return Ok(());
        }
        let ptr = data.as_mut_ptr();
        // SAFETY: one exclusive slice provides count whole readable/writable
        // blocks. C explicitly supports src == dst, preserving ciphertext before
        // CBC decryption writes it. The initialized context and optional IV are
        // borrowed without writers and disjoint; no pointer is retained.
        // C never writes the schedule despite its mutable ABI pointer.
        unsafe {
            ffi::av_cast5_crypt2(
                self.as_ptr().cast_mut(),
                ptr,
                ptr,
                count,
                iv.map_or(core::ptr::null_mut(), |iv| iv.as_mut_ptr()),
                direction.as_int(),
            )
        };
        Ok(())
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn rejected_buffers_are_atomic() {
        let key = [0; 5];
        let context = Cast5::new(&key).unwrap();
        let mut data = [0x99; 8 * 2];
        let mut iv = [0x77; 8];
        assert_eq!(
            context
                .as_ref()
                .crypt_cbc(&mut data, &[0; 8], Some(&mut iv), Direction::Encrypt),
            Err(CryptError::LengthMismatch)
        );
        assert_eq!(
            context.as_ref().crypt_cbc_in_place(
                &mut data[..8 - 1],
                Some(&mut iv),
                Direction::Decrypt
            ),
            Err(CryptError::IncompleteBlock)
        );
        assert_eq!(
            context
                .as_ref()
                .crypt(&mut data, &[0; 8], Direction::Encrypt),
            Err(CryptError::LengthMismatch)
        );
        assert_eq!(
            context
                .as_ref()
                .crypt_in_place(&mut data[..7], Direction::Decrypt),
            Err(CryptError::IncompleteBlock)
        );
        assert_eq!(data, [0x99; 8 * 2]);
        assert_eq!(iv, [0x77; 8]);
    }

    #[test]
    fn rejects_invalid_lengths_before_ffi() {
        for len in [0, 1, 4, 17, 128] {
            assert!(matches!(
                Cast5::new(&vec![0; len]),
                Err(InitError::InvalidKeyLength)
            ));
        }
        assert_eq!(key_bits(&[0; 5]), Ok(40));
        assert_eq!(key_bits(&[0; 16]), Ok(128));
    }

    #[test]
    fn errors_describe_the_failure() {
        assert!(
            InitError::InvalidKeyLength
                .to_string()
                .contains("5 through 16")
        );
        assert!(InitError::AllocationFailed.to_string().contains("allocate"));
        assert_eq!(
            InitError::C(-22).to_string(),
            "CAST5 initialization failed: -22"
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn raw_owner_round_trip_and_borrowed_handles() {
        let owner = Cast5::new(&[0; 5]).unwrap();
        let ptr = owner.into_c();
        // SAFETY: into_c transferred the sole live initialized allocation;
        // no handles remain, and the same Cast5Free policy adopts it once.
        let mut owner = unsafe { Cast5Box::from_c(ptr) }.unwrap();
        let shared = owner.as_ref();
        assert_eq!(shared.as_ptr(), ptr);
        let mut exclusive = owner.as_mut();
        assert_eq!(exclusive.as_mut_ptr(), ptr);
        assert_eq!(exclusive.as_ref().as_ptr(), ptr);
        exclusive.as_mut().init(&[0xff; 16]).unwrap();
    }
}
