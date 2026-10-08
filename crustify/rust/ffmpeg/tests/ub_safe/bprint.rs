//! UB tests of `libavutil/bprint.h`'s safe [`PaddedStruct`] constants: they
//! evaluate at compile time, without overflow, and describe the C structure's
//! storage.

use core::mem::{align_of, size_of};

use ffmpeg::libavutil::bprint::PaddedStruct;
use ffmpeg::libavutil::ffi;

type BPrint = ffi::AVBPrint;

// Evaluated in const context: an underflow here fails the build.
const SIZE: usize = <BPrint as PaddedStruct>::SIZE;
const FIELDS_SIZE: usize = <BPrint as PaddedStruct>::FIELDS_SIZE;
const PADDING_LEN: usize = <BPrint as PaddedStruct>::PADDING_LEN;

#[test]
fn constants_describe_the_storage() {
    assert_eq!(FIELDS_SIZE + PADDING_LEN, SIZE);
    assert_eq!(size_of::<BPrint>(), SIZE);
    assert_eq!(SIZE % align_of::<BPrint>(), 0);
    assert_eq!(FIELDS_SIZE % align_of::<BPrint>(), 0);
}
