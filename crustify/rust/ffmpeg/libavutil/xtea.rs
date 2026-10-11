//! Safe layout and lifecycle for `libavutil/xtea.h`.
//!
//! The public layout has sixteen key words, although C's initializers and
//! cipher routines use only the first four. Initialization preserves the
//! remaining twelve words. No pointers or resources are stored in the object.
//! Inline values need no teardown; [`XteaBox`] uses `av_xtea_alloc` and the
//! existing [`Heap`] policy's `av_free`. There is no C refcount or cloner;
//! copies of the words are independent values, and heap clones use `av_memdup`.
//! Cipher operations borrow the context immutably: despite C's mutable pointer
//! signature, all cipher paths only read its key. Buffers and CBC state are
//! borrowed for the call and never retained. No `Send`/`Sync` assertion is added.

use core::marker::PhantomData;
use core::ptr::{NonNull, addr_of, addr_of_mut};

use ffibox::{CNew, CZeroable, define_ctype};

use super::ffi;
use super::mem::{Heap, HeapBox};

/// Whether to encrypt plaintext or decrypt ciphertext.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Direction {
    Encrypt,
    Decrypt,
}

/// A rejected buffer shape. Rejection changes neither output nor CBC state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CryptError {
    /// Separate input and output must have equal byte lengths.
    LengthMismatch,
    /// The buffer must contain a whole number of eight-byte blocks.
    PartialBlock,
    /// The number of blocks cannot be represented by C's `int` count.
    TooManyBlocks,
}

impl core::fmt::Display for CryptError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Self::LengthMismatch => "XTEA input and output lengths differ",
            Self::PartialBlock => "XTEA requires whole eight-byte blocks",
            Self::TooManyBlocks => "XTEA block count exceeds C int",
        })
    }
}

impl std::error::Error for CryptError {}

fn block_count(dst_len: usize, src_len: usize) -> Result<i32, CryptError> {
    if dst_len != src_len {
        return Err(CryptError::LengthMismatch);
    }
    if !dst_len.is_multiple_of(8) {
        return Err(CryptError::PartialBlock);
    }
    i32::try_from(dst_len / 8).map_err(|_| CryptError::TooManyBlocks)
}

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
    /// Wraps: av_xtea_crypt
    ///
    /// Process equal-length buffers using big-endian block words.
    /// `None` selects ECB; `Some(iv)` selects CBC and replaces the IV with
    /// the last ciphertext block. Empty buffers leave the IV unchanged.
    /// No padding is added. Use [`Self::crypt_in_place`] for exact overlap.
    /// Key-word byte order is selected independently by the initializer.
    pub fn crypt(
        &self,
        dst: &mut [u8],
        src: &[u8],
        iv: Option<&mut [u8; 8]>,
        direction: Direction,
    ) -> Result<(), CryptError> {
        self.process(dst, Some(src), iv, direction, false)
    }

    /// Process big-endian blocks in place, with the same CBC and length
    /// contracts as [`Self::crypt`]. CBC decryption saves each ciphertext
    /// block before overwriting it, so the IV advances correctly.
    pub fn crypt_in_place(
        &self,
        data: &mut [u8],
        iv: Option<&mut [u8; 8]>,
        direction: Direction,
    ) -> Result<(), CryptError> {
        self.process(data, None, iv, direction, false)
    }

    /// Wraps: av_xtea_le_crypt
    ///
    /// Process equal-length buffers using little-endian block words.
    /// The ECB/CBC, IV, padding and length contracts are those of [`Self::crypt`].
    /// Key-word byte order is selected independently by the initializer.
    pub fn crypt_le(
        &self,
        dst: &mut [u8],
        src: &[u8],
        iv: Option<&mut [u8; 8]>,
        direction: Direction,
    ) -> Result<(), CryptError> {
        self.process(dst, Some(src), iv, direction, true)
    }

    /// Process little-endian blocks in place; see [`Self::crypt_le`].
    pub fn crypt_le_in_place(
        &self,
        data: &mut [u8],
        iv: Option<&mut [u8; 8]>,
        direction: Direction,
    ) -> Result<(), CryptError> {
        self.process(data, None, iv, direction, true)
    }

    fn process(
        &self,
        dst: &mut [u8],
        src: Option<&[u8]>,
        iv: Option<&mut [u8; 8]>,
        direction: Direction,
        le: bool,
    ) -> Result<(), CryptError> {
        let count = block_count(dst.len(), src.map_or(dst.len(), <[u8]>::len))?;
        let src_ptr = src.map_or_else(|| dst.as_ptr(), <[u8]>::as_ptr);
        let iv_ptr = iv.map_or(core::ptr::null_mut(), |iv| iv.as_mut_ptr());
        let crypt = if le {
            ffi::av_xtea_le_crypt
        } else {
            ffi::av_xtea_crypt
        };
        // SAFETY: both C cipher paths only READ the live context's key, so
        // casting its shared pointer to mutable does not write shared storage.
        // Checked buffers cover count*8 bytes and either do not overlap or
        // overlap exactly (supported by C). The exclusive IV is NULL or eight
        // disjoint bytes. Zero count accesses nothing; C retains no pointers.
        unsafe {
            crypt(
                self.as_ptr().cast_mut(),
                dst.as_mut_ptr(),
                src_ptr,
                count,
                iv_ptr,
                i32::from(direction == Direction::Decrypt),
            )
        }
        Ok(())
    }

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
    fn buffer_validation_and_errors() {
        assert_eq!(block_count(8, 16), Err(CryptError::LengthMismatch));
        for len in 0..=65 {
            let expected = if len % 8 == 0 {
                Ok((len / 8) as i32)
            } else {
                Err(CryptError::PartialBlock)
            };
            assert_eq!(block_count(len, len), expected);
        }
        if usize::BITS > 32 {
            let max = (i32::MAX as usize) * 8;
            assert_eq!(block_count(max, max), Ok(i32::MAX));
            assert_eq!(
                block_count(max + 8, max + 8),
                Err(CryptError::TooManyBlocks)
            );
        }
        for error in [
            CryptError::LengthMismatch,
            CryptError::PartialBlock,
            CryptError::TooManyBlocks,
        ] {
            assert!(!error.to_string().is_empty());
        }
    }

    #[test]
    fn rejected_buffers_preserve_output_iv_and_key() {
        let context = Xtea::from_key_words([u32::MAX; 16]);
        let view = context.as_ref();
        for le in [false, true] {
            for direction in [Direction::Encrypt, Direction::Decrypt] {
                for len in 0..=17 {
                    let mut dst = vec![0xa5; len];
                    let mut iv = [0x5a; 8];
                    assert_eq!(
                        view.process(&mut dst, Some(&[0; 24]), Some(&mut iv), direction, le),
                        Err(CryptError::LengthMismatch)
                    );
                    assert_eq!(dst, vec![0xa5; len]);
                    assert_eq!(iv, [0x5a; 8]);
                    if len % 8 != 0 {
                        assert_eq!(
                            view.process(&mut dst, None, Some(&mut iv), direction, le),
                            Err(CryptError::PartialBlock)
                        );
                        assert_eq!(
                            view.process(
                                &mut dst,
                                Some(&vec![0; len]),
                                Some(&mut iv),
                                direction,
                                le
                            ),
                            Err(CryptError::PartialBlock)
                        );
                        assert_eq!(dst, vec![0xa5; len]);
                        assert_eq!(iv, [0x5a; 8]);
                    }
                }
            }
        }
        assert_eq!(view.key(), [u32::MAX; 16]);
    }

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
