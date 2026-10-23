use std::cell::RefCell;

use ffmpeg_bindings::libavutil::libavutil::hwcontext::{FramesContextMut, UninitFrames};

thread_local! {
    static KEPT: RefCell<Option<FramesContextMut<'static>>> = const { RefCell::new(None) };
}

fn install(mut frames: UninitFrames) {
    let _ = frames
        .context_mut()
        .set_free_callback(|ctx: FramesContextMut<'_>| KEPT.with(|k| *k.borrow_mut() = Some(ctx)));
}

fn main() {}
