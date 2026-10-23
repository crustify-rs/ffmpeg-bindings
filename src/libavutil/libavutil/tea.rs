//! Owned and borrowed views of the opaque TEA context.
//!
//! C keeps the layout private. The context contains only inline key words and
//! a round count; no key pointer is retained and there is no clone primitive.
//! Storage remains C-allocated and is released by the [`TeaFree`] policy.

use core::ptr::NonNull;

use ffibox::{CBox, CDrop, define_ctype};

use super::aes::{CryptError, Direction};
use super::ffi;
use super::mem::Heap;

define_ctype!(
    /// Wraps: AVTEA
    ///
    /// An initialized TEA context. Its private C fields are not exposed.
    /// No `Send`/`Sync` guarantee is added.
    ///
    /// Handles cannot outlive their owner:
    /// ```compile_fail
    /// use ffmpeg_bindings::libavutil::libavutil::tea::{Tea, TeaRef};
    /// fn dangling() -> TeaRef<'static> {
    ///     let owner = Tea::new(&[0; 16], 64).unwrap();
    ///     owner.as_ref()
    /// }
    /// ```
    /// Exclusive handles cannot alias:
    /// ```compile_fail
    /// use ffmpeg_bindings::libavutil::libavutil::tea::Tea;
    /// let mut owner = Tea::new(&[0; 16], 64).unwrap();
    /// let mut first = owner.as_mut();
    /// let mut second = owner.as_mut();
    /// first.init(&[1; 16], 64);
    /// second.init(&[2; 16], 64);
    /// ```
    Tea,
    TeaRef,
    TeaMut,
    ffi::AVTEA
);

/// Sole ownership of a C-allocated TEA context, freed with `av_free`.
///
/// The opaque layout has no Rust size, so there is no Rust-sized allocation:
///
/// ```compile_fail,E0277
/// use ffmpeg_bindings::libavutil::libavutil::tea::TeaBox;
/// let _ = TeaBox::new_uninit();
/// ```
pub type TeaBox = CBox<Tea, TeaFree>;

/// Storage-only release of an `av_tea_alloc` context. Unlike [`Heap`], it
/// offers no generic allocator: the opaque layout's Rust size is zero, so
/// only C can size a TEA context.
#[derive(Clone, Copy, Debug, Default)]
pub struct TeaFree;

/// Wraps: `av_free`
// SAFETY: TeaBox adopts only `av_tea_alloc` storage, which owns no sub-allocations;
// Heap's av_free releases that av_malloc-family block once without reading it.
unsafe impl CDrop<Tea> for TeaFree {
    unsafe fn c_drop(&self, ptr: NonNull<Tea>) {
        // SAFETY: CDrop's caller transfers the unique av_malloc-family claim.
        unsafe { <Heap as CDrop<Tea>>::c_drop(&Heap, ptr) }
    }
}

impl Tea {
    /// Wraps: `av_tea_alloc`
    ///
    /// Allocate and initialize a context; `None` means allocation failed.
    /// `rounds` is passed unchanged (64 is the standard round count).
    /// The key is copied, so its borrow ends when this call returns.
    #[must_use]
    pub fn new(key: &[u8; 16], rounds: i32) -> Option<TeaBox> {
        // SAFETY: as the comment below states, for this part of the operation.
        let tea_alloc = unsafe { ffi::av_tea_alloc() };
        // SAFETY: the allocator returns fresh, zero-filled C storage with no
        // nested resources. TeaFree frees it with the paired av_free. The owner
        // stays private until init has established the context's key/rounds.
        let mut owner = unsafe { TeaBox::from_c(tea_alloc) }?;
        owner.as_mut().init(key, rounds);
        Some(owner)
    }
}

impl TeaMut<'_> {
    /// Wraps: `av_tea_init`
    ///
    /// Replace the key and round count. C copies the 16 key bytes and retains
    /// no pointer to the key. All signed round counts are accepted by C.
    pub fn init(&mut self, key: &[u8; 16], rounds: i32) {
        // SAFETY: the handle exclusively borrows a full C allocation. C writes
        // only that context and reads exactly the 16 live key bytes, retaining
        // neither pointer. The scalar round count is stored without arithmetic.
        unsafe { ffi::av_tea_init(self.as_mut_ptr(), key.as_ptr(), rounds) }
    }
}

/// Wraps: `av_tea_size`
///
/// The C context's size in bytes, borrowed from immutable C static storage.
#[must_use]
pub fn context_size() -> &'static i32 {
    // SAFETY: C defines this global as `const int = sizeof(context)`; it has
    // static lifetime and no writer anywhere in the library.
    unsafe { &ffi::av_tea_size }
}

fn crypt_count(input_len: usize, output_len: usize) -> Result<i32, CryptError> {
    if output_len < input_len {
        return Err(CryptError::LengthMismatch);
    }
    if !input_len.is_multiple_of(8) {
        return Err(CryptError::IncompleteBlock);
    }
    i32::try_from(input_len / 8).map_err(|_| CryptError::CountOverflow)
}

impl TeaRef<'_> {
    /// Wraps: `av_tea_crypt`
    ///
    /// Encrypt or decrypt whole 8-byte blocks. A mutable
    /// IV selects CBC and is updated after each block; `None` selects ECB.
    /// Output bytes beyond `src.len()` remain untouched. Partial blocks, short
    /// outputs and block counts above C `INT_MAX` return an error before any mutation.
    ///
    /// # Errors
    ///
    /// Returns a [`CryptError`] when it rejects the buffers, leaving the context, output and IV
    /// unchanged.
    pub fn crypt(
        &self,
        dst: &mut [u8],
        src: &[u8],
        iv: Option<&mut [u8; 8]>,
        direction: Direction,
    ) -> Result<(), CryptError> {
        let count = crypt_count(src.len(), dst.len())?;
        // SAFETY: the shared initialized schedule is read-only throughout C.
        // Validation covers count blocks in disjoint input/output slices. The optional IV is one
        // exclusive block and cannot alias either slice. C retains no pointers.
        unsafe {
            ffi::av_tea_crypt(
                self.as_ptr().cast_mut(),
                dst.as_mut_ptr(),
                src.as_ptr(),
                count,
                iv.map_or(core::ptr::null_mut(), |iv| iv.as_mut_ptr()),
                direction.as_int(),
            );
        };
        Ok(())
    }

    /// The in-place form of `crypt`, with the same validation and IV semantics.
    ///
    /// The IV cannot overlap the data:
    /// ```compile_fail,E0499
    /// use ffmpeg_bindings::libavutil::libavutil::{aes::Direction, tea::Tea};
    /// let mut cipher = Tea::new(&[0; 16], 64).unwrap();
    /// let mut bytes = [0; 8];
    /// let direction = Direction::Encrypt;
    /// cipher.as_ref().crypt_in_place(&mut bytes, Some(&mut bytes), direction).unwrap();
    /// ```
    ///
    /// # Errors
    ///
    /// Returns a [`CryptError`] when it rejects the buffers, leaving the context, output and IV
    /// unchanged.
    pub fn crypt_in_place(
        &self,
        data: &mut [u8],
        iv: Option<&mut [u8; 8]>,
        direction: Direction,
    ) -> Result<(), CryptError> {
        let count = crypt_count(data.len(), data.len())?;
        // SAFETY: the shared schedule is read-only in C; data holds full writable
        // blocks. C supports exact overlap, saving ciphertext before overwriting in
        // CBC decryption. IV is a disjoint exclusive block; no pointer is retained.
        unsafe {
            ffi::av_tea_crypt(
                self.as_ptr().cast_mut(),
                data.as_mut_ptr(),
                data.as_ptr(),
                count,
                iv.map_or(core::ptr::null_mut(), |iv| iv.as_mut_ptr()),
                direction.as_int(),
            );
        };
        Ok(())
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    #[test]
    fn crypt_length_boundaries() {
        assert_eq!(crypt_count(0, 0), Ok(0));
        assert_eq!(crypt_count(8 - 1, 8), Err(CryptError::IncompleteBlock));
        assert_eq!(crypt_count(8, 8 - 1), Err(CryptError::LengthMismatch));
        let max = i32::MAX as usize * 8;
        assert_eq!(crypt_count(max, usize::MAX), Ok(i32::MAX));
        assert_eq!(
            crypt_count(max + 8, usize::MAX),
            Err(CryptError::CountOverflow)
        );
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn rejected_buffers_are_unchanged() {
        let mut owner = Tea::new(&[0; 16], 64).unwrap();
        let mut output = [0x7B; 8];
        let mut iv = [0x8C; 8];
        assert_eq!(
            owner.as_mut().as_ref().crypt(
                &mut output,
                &[0; 8 - 1],
                Some(&mut iv),
                Direction::Encrypt
            ),
            Err(CryptError::IncompleteBlock)
        );
        assert_eq!(
            owner.as_mut().as_ref().crypt(
                &mut output[..1],
                &[0; 8],
                Some(&mut iv),
                Direction::Decrypt
            ),
            Err(CryptError::LengthMismatch)
        );
        assert_eq!(
            owner.as_mut().as_ref().crypt_in_place(
                &mut output[..1],
                Some(&mut iv),
                Direction::Decrypt
            ),
            Err(CryptError::IncompleteBlock)
        );
        assert_eq!(output, [0x7B; 8]);
        assert_eq!(iv, [0x8C; 8]);
    }

    #[test]
    fn layout_matches_ffi() {
        assert_eq!(size_of::<Tea>(), size_of::<ffi::AVTEA>());
        assert_eq!(align_of::<Tea>(), align_of::<ffi::AVTEA>());
        assert_eq!(size_of::<TeaRef<'_>>(), size_of::<usize>());
        assert_eq!(size_of::<TeaMut<'_>>(), size_of::<usize>());
    }

    #[test]
    #[cfg_attr(miri, ignore = "calls C")]
    fn raw_ownership_round_trip() {
        let owner = Tea::new(&[7; 16], 64).unwrap();
        let ptr = owner.into_c();
        // SAFETY: into_c transferred the sole live allocation, still initialized
        // and paired with TeaFree's av_free; no borrow remains.
        let owner = unsafe { TeaBox::from_c(ptr) }.unwrap();
        drop(owner);
    }
}
