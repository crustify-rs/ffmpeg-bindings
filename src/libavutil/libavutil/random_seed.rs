//! Safe wrappers over `libavutil/random_seed.h`.

use super::ffi;

/// Wraps: av_get_random_seed
///
/// A best-effort random 32-bit seed for a faster PRNG: from the system's
/// entropy source, or, failing that, from timing jitter. It can be slow; call
/// it again for more bits.
#[must_use]
pub fn get() -> u32 {
    // SAFETY: `av_get_random_seed` takes no arguments and has no
    // preconditions; it fills a local of its own.
    unsafe { ffi::av_get_random_seed() }
}
