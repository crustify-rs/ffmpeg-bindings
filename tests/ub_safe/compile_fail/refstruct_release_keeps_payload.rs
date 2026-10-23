use std::sync::{Arc, Mutex};

use ffmpeg_bindings::libavutil::libavutil::intfloat_h::{IntFloat64, IntFloat64Mut};
use ffmpeg_bindings::libavutil::libavutil::refstruct;

fn main() {
    // The payload handle kept past the release would reach the block after
    // C frees it.
    let kept: Arc<Mutex<Option<IntFloat64Mut<'static>>>> = Arc::new(Mutex::new(None));
    let slot = Arc::clone(&kept);
    let _ = refstruct::alloc_with_release::<IntFloat64, _>(move |last| {
        *slot.lock().unwrap() = Some(last);
    });
}
