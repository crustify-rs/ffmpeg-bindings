//! UB tests for `libavutil/version.c`'s version queries.

use std::thread;

use ffmpeg_bindings::libavutil::libavutil::version::{
    configuration, license, version, version_info,
};

#[test]
#[cfg_attr(miri, ignore = "calls C")]
fn static_strings_read_whole_and_cross_threads() {
    let lens = |_| {
        (
            version_info().count_bytes(),
            configuration().count_bytes(),
            license().count_bytes(),
        )
    };
    let theirs = thread::spawn(move || (0..100).map(lens).collect::<Vec<_>>())
        .join()
        .unwrap();
    assert!(theirs.iter().all(|&l| l == lens(0)));
    let strings: &'static [u8] = version_info().to_bytes();
    assert!(!strings.is_empty() && version() > 0);
}
