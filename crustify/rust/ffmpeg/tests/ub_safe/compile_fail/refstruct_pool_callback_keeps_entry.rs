use std::sync::Mutex;

use ffmpeg::libavutil::intfloat::{IntFloat64, IntFloat64Mut};
use ffmpeg::libavutil::refstruct::PoolCallbacks;

fn main() {
    // An entry handle kept past the callback would reach the entry after C
    // hands it to its next user.
    let kept: Mutex<Vec<IntFloat64Mut<'static>>> = Mutex::new(Vec::new());
    let _ = PoolCallbacks::<IntFloat64, _>::new(kept).reset(|kept, entry| {
        kept.lock().unwrap().push(entry);
    });
}
