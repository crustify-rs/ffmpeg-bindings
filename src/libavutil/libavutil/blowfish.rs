//! Safe layouts and lifecycle for `libavutil/blowfish.h`.
//!
//! The schedule owns no pointers: inline values and C-allocated boxes both
//! support shared and exclusive handles. Array getters return snapshots, not
//! references to memory C can access. Dropping a box uses [`Heap`](super::mem::Heap).

use super::aes::{CryptError, Direction, buffer_count};
use super::{ffi, mem::HeapBox};
use core::ptr::{addr_of, addr_of_mut};
use ffibox::{CZeroable, define_ctype};

define_ctype!(
    /// Wraps: AVBlowfish
    ///
    /// A resource-free Blowfish key schedule. Field replacement accepts any
    /// words (all bit patterns are valid), but changes the resulting cipher.
    #[derive(Clone, Copy)]
    Blowfish, BlowfishRef, BlowfishMut, ffi::AVBlowfish
);

// SAFETY: both fields contain only u32 arrays. Zero is valid for every field
// accessor and C crypt operation (though not a meaningful keyed schedule).
// The object owns no resources and av_free does not inspect its contents.
unsafe impl CZeroable for Blowfish {}

/// A sole C-allocated schedule, freed by `av_free`; cloning copies its words.
pub type BlowfishBox = HeapBox<Blowfish>;

/// Failure to construct or initialize a schedule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitError {
    /// C reads at least one byte, and its key length must fit an `int`.
    InvalidKeyLength,
    /// C could not allocate a context.
    AllocationFailed,
}

fn key_len(key: &[u8]) -> Result<i32, InitError> {
    if key.is_empty() {
        return Err(InitError::InvalidKeyLength);
    }
    i32::try_from(key.len()).map_err(|_| InitError::InvalidKeyLength)
}

impl Blowfish {
    /// Initialize a schedule in Rust-owned inline storage.
    ///
    /// Matches C for any nonempty key whose length fits `i32`; C does not
    /// enforce the customary Blowfish key-size limit. The key is not retained.
    pub fn new(key: &[u8]) -> Result<Self, InitError> {
        key_len(key)?;
        let mut context = Self::zeroed();
        context.as_mut().init(key)?;
        Ok(context)
    }

    /// Wraps: av_blowfish_alloc
    ///
    /// Allocate and initialize a sole owner; the key is not retained.
    pub fn new_boxed(key: &[u8]) -> Result<BlowfishBox, InitError> {
        key_len(key)?;
        // SAFETY: C returns a fresh av_mallocz block with two initialized u32
        // arrays, or NULL. Heap frees it with av_free and C retains no pointer.
        let mut context = unsafe { BlowfishBox::from_c(ffi::av_blowfish_alloc()) }
            .ok_or(InitError::AllocationFailed)?;
        context.as_mut().init(key)?;
        Ok(context)
    }
}

impl BlowfishRef<'_> {
    /// Field: AVBlowfish.p
    ///
    /// Copy the 18 round-key words.
    pub fn p(&self) -> [u32; 18] {
        // SAFETY: the shared handle keeps the initialized array alive and
        // excludes writes for this read; projection creates no C reference.
        unsafe { addr_of!((*self.as_ptr()).p).read() }
    }

    /// Field: AVBlowfish.s
    ///
    /// Copy all four 256-word substitution tables.
    pub fn s(&self) -> [[u32; 256]; 4] {
        // SAFETY: as p(), for the complete initialized s array.
        unsafe { addr_of!((*self.as_ptr()).s).read() }
    }
}

impl BlowfishMut<'_> {
    /// Wraps: av_blowfish_init
    ///
    /// Replace the whole schedule. An invalid key leaves the context unchanged.
    pub fn init(&mut self, key: &[u8]) -> Result<(), InitError> {
        let len = key_len(key)?;
        // SAFETY: the exclusive handle addresses a full writable context. C
        // reads key[0..len] cyclically, with len positive and representable,
        // fills both arrays, and retains neither pointer after the call.
        unsafe { ffi::av_blowfish_init(self.as_mut_ptr(), key.as_ptr(), len) };
        Ok(())
    }

    /// Field: AVBlowfish.p
    ///
    /// Replace the round-key words without changing the substitution tables.
    pub fn set_p(&mut self, words: [u32; 18]) {
        // SAFETY: exclusive access to the complete p array; every u32 is valid.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).p).write(words) }
    }

    /// Field: AVBlowfish.s
    ///
    /// Replace the substitution tables without changing the round-key words.
    pub fn set_s(&mut self, tables: [[u32; 256]; 4]) {
        // SAFETY: exclusive access to the complete s array; every u32 is valid.
        unsafe { addr_of_mut!((*self.as_mut_ptr()).s).write(tables) }
    }
}

impl BlowfishRef<'_> {
    /// Wraps: av_blowfish_crypt
    ///
    /// Encrypt or decrypt whole blocks.
    /// `None` selects ECB; `Some` selects CBC and updates the IV after each block.
    /// Buffers must have equal lengths; errors leave all state unchanged.
    pub fn crypt(
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
            ffi::av_blowfish_crypt(
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

    /// In-place form of [`Self::crypt`], with the same modes and validation.
    pub fn crypt_in_place(
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
            ffi::av_blowfish_crypt(
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

impl BlowfishRef<'_> {
    /// Wraps: av_blowfish_crypt_ecb
    ///
    /// Process a pair of numeric 32-bit halves (not host-endian byte strings).
    pub fn crypt_ecb(&self, halves: &mut [u32; 2], direction: Direction) {
        let ptr = halves.as_mut_ptr();
        // SAFETY: the context is initialized and shared without writers. The
        // two distinct aligned u32 slots are writable for the synchronous call;
        // C reads both before writing and retains neither pointer.
        unsafe {
            ffi::av_blowfish_crypt_ecb(
                self.as_ptr().cast_mut(),
                ptr,
                ptr.add(1),
                direction.as_int(),
            )
        };
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    // No FFI: also run under Miri to check layout, projection and reborrows.
    #[test]
    #[cfg(not(miri))]
    fn rejected_buffers_are_atomic() {
        let key = [0; 1];
        let context = Blowfish::new(&key).unwrap();
        let mut data = [0x99; 8 * 2];
        let mut iv = [0x77; 8];
        assert_eq!(
            context
                .as_ref()
                .crypt(&mut data, &[0; 8], Some(&mut iv), Direction::Encrypt),
            Err(CryptError::LengthMismatch)
        );
        assert_eq!(
            context
                .as_ref()
                .crypt_in_place(&mut data[..8 - 1], Some(&mut iv), Direction::Decrypt),
            Err(CryptError::IncompleteBlock)
        );
        assert_eq!(data, [0x99; 8 * 2]);
        assert_eq!(iv, [0x77; 8]);
    }

    #[test]
    fn layout_and_field_projections() {
        assert_eq!(size_of::<Blowfish>(), size_of::<ffi::AVBlowfish>());
        assert_eq!(align_of::<Blowfish>(), align_of::<ffi::AVBlowfish>());
        let mut context = Blowfish::zeroed();
        let mut handle = context.as_mut();
        handle.as_mut().set_p([u32::MAX; 18]);
        handle.set_s([[0x1234abcd; 256]; 4]);
        assert_eq!(handle.as_ref().p(), [u32::MAX; 18]);
        assert_eq!(handle.as_ref().s(), [[0x1234abcd; 256]; 4]);
        let copy = context;
        context.as_mut().set_p([0; 18]);
        assert_eq!(copy.as_ref().p(), [u32::MAX; 18]);
    }

    #[test]
    fn invalid_key_preserves_schedule() {
        assert!(matches!(
            Blowfish::new(&[]),
            Err(InitError::InvalidKeyLength)
        ));
        assert!(matches!(
            Blowfish::new_boxed(&[]),
            Err(InitError::InvalidKeyLength)
        ));
        let mut context = Blowfish::zeroed();
        context.as_mut().set_p([42; 18]);
        assert_eq!(context.as_mut().init(&[]), Err(InitError::InvalidKeyLength));
        assert_eq!(context.as_ref().p(), [42; 18]);
    }

    #[test]
    fn raw_owner_round_trip() {
        let context = Blowfish::new_boxed(b"key").unwrap();
        let expected = context.as_ref().p();
        let ptr = context.into_c();
        // SAFETY: ptr came from this owner's into_c; no intervening access or
        // free occurred, and its unique ownership is transferred back once.
        let context = unsafe { BlowfishBox::from_c(ptr) }.unwrap();
        assert_eq!(context.as_ref().p(), expected);
    }
}
