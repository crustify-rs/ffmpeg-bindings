//! A borrowed run of `Rational` (a `CSlice<'_, Rational>`, the view every
//! `HeapVec<Rational>` and every C array field of `AVRational` hands out)
//! cannot be copied into an owned `HeapVec<Rational>` with
//! `CSlice::to_cvec`: ffibox implements the method for `CPlainElem` elements
//! only, although it asks for nothing but `T: Copy`, which `Rational` is, and
//! an owned `HeapVec<Rational>` already clones through the same byte copy.
//!
//! On the affected ffibox (`main` at `695d52d`) this program does not
//! compile, so `cargo run` exits non-zero; against the patched branch it
//! builds, runs and prints the copied terms.
#![forbid(unsafe_code)]

use ffmpeg::libavutil::mem::{Heap, HeapVec};
use ffmpeg::libavutil::rational::Rational;

fn main() {
    let rates = [Rational::new(24000, 1001), Rational::new(25, 1), Rational::new(0, 0)];
    // An owned array of `Rational` in an `av_malloc` block: it clones.
    let owned = HeapVec::from_slice(&rates).unwrap();
    let cloned = owned.clone();
    // The same run, borrowed as a `CSlice<'_, Rational>`, does not copy.
    let copied: HeapVec<Rational> = owned.as_handles().to_cvec::<Heap>().unwrap();
    for (c, k) in cloned.as_handles().iter().zip(copied.as_handles().iter()) {
        println!("{}/{} == {}/{}", c.num(), c.den(), k.num(), k.den());
        assert_eq!((c.num(), c.den()), (k.num(), k.den()));
    }
}
